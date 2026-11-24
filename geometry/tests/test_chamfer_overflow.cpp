// SPDX-License-Identifier: MIT
// Chamfers wider than a face beside them (mitcad#240, mitcad#263), with the
// OCCT port's patches 0140 and later (third_party/vcpkg-ports/opencascade;
// a groove's end wall chamfered past its flanks, 0671, mitcad#235; a boss's
// foot past a slanted plate corner, 0941, mitcad#495),
// on synthetic shapes and OCCT's chamfer alone. The cross-sections are
// exact, so are the volumes.

#include <cmath>
#include <cstdio>
#include <algorithm>
#include <array>
#include <functional>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepFilletAPI_MakeFillet2d.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Surface.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/analysis/properties.hpp"
#include "mitcad/geometry/dressup.hpp"

namespace test {
namespace {

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1.0e-9);
  return props.Mass();
}

// The volume by the analysis library's integrals, which follow the spans
// of the faces' curves (OCCT's adaptive rule leaves 1e-6 on a chamfer's
// cone in one face of eleven edges).
double exact_volume_of(const TopoDS_Shape& shape) {
  return mitcad::analysis::physical_properties(shape).volume;
}

// Whether every edge's p-curve on each of its faces lies on the edge's 3d
// curve (sampled): the checker passes wires whose p-curves on a seam's other
// side were taken in the wrong period by luck of their order.
bool pcurves_fit(const TopoDS_Shape& shape) {
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      double f1 = 0.0, l1 = 0.0, f2 = 0.0, l2 = 0.0;
      const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, f1, l1);
      const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, f2, l2);
      if (pcurve.IsNull() || curve.IsNull()) {
        return false;
      }
      for (int k = 0; k <= 4; ++k) {
        const double t = f1 + (l1 - f1) * k / 4.0;
        const gp_Pnt2d uv = pcurve->Value(t);
        if (surface->Value(uv.X(), uv.Y()).Distance(curve->Value(t)) > 1e-4 + BRep_Tool::Tolerance(edge)) {
          return false;
        }
      }
    }
  }
  return true;
}

int faces_of(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer e(shape, TopAbs_FACE); e.More(); e.Next()) {
    ++n;
  }
  return n;
}

TopoDS_Shape cut(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepAlgoAPI_Cut op(a, b);
  return op.Shape();
}

TopoDS_Shape fuse(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepAlgoAPI_Fuse op(a, b);
  op.SimplifyResult();
  return op.Shape();
}

// The edges of a shape all of whose sampled points satisfy `on`.
std::vector<TopoDS_Edge> edges_where(const TopoDS_Shape& shape, const std::function<bool(const gp_Pnt&)>& on) {
  std::vector<TopoDS_Edge> found;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(shape, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges(i));
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    BRepAdaptor_Curve curve(edge);
    bool all = true;
    for (int k = 0; k <= 8 && all; ++k) {
      all = on(curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * k / 8.0));
    }
    if (all) {
      found.push_back(edge);
    }
  }
  return found;
}

// OCCT's chamfer of the edges with one distance: the result, or a null
// shape when it fails.
TopoDS_Shape chamfered(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges, double distance) {
  BRepFilletAPI_MakeChamfer maker(body);
  for (const TopoDS_Edge& edge : edges) {
    if (maker.Contour(edge) == 0) {
      maker.Add(distance, edge);
    }
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "chamfer overflow test: %s\n", failure.what());
    return TopoDS_Shape();
  }
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// A block with a hole of radius 1 from its top down to a step 1.5 below,
// and under the step a counterbore of radius 2 from the bottom: the step is
// a ring 1 wide facing down. Its inner rim is a whole circle or, `split`,
// two arcs (a vertex at 188.6 degrees); the counterbore's seam lies at the
// rim's vertex (180 degrees) or, `turned`, at 90 degrees.
TopoDS_Shape step_ring(bool split, bool turned) {
  const gp_Dir up(0, 0, 1);
  TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, -6), gp_Pnt(10, 10, 0)).Shape();
  block = cut(block, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -2), up, gp_Dir(-1, 0, 0)), 1.0, 3).Shape());
  block = cut(block,
              BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -7), up, turned ? gp_Dir(0, 1, 0) : gp_Dir(-1, 0, 0)), 2.0,
                                       5.5)
                  .Shape());
  if (split) {
    const double a = 188.6 * kPi / 180.0;
    BRepAlgoAPI_Splitter splitter;
    NCollection_List<TopoDS_Shape> arguments;
    NCollection_List<TopoDS_Shape> tools;
    arguments.Append(block);
    tools.Append(BRepBuilderAPI_MakeVertex(gp_Pnt(std::cos(a), std::sin(a), -1.5)).Vertex());
    splitter.SetArguments(arguments);
    splitter.SetTools(tools);
    splitter.Build();
    block = TopExp_Explorer(splitter.Shape(), TopAbs_SOLID).Current();
  }
  return block;
}

// The step ring's inner rim: one circle or, split, two arcs.
std::vector<TopoDS_Edge> step_ring_rim(const TopoDS_Shape& body) {
  return edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() + 1.5) < 1e-6 && std::abs(std::hypot(p.X(), p.Y()) - 1.0) < 1e-6;
  });
}

// The step's inner rim chamfered by 1, the ring's width (patch 0140): the
// chamfer's cone runs from the hole's wall 1 above the step to the
// counterbore's wall, the ring vanishes. The triangle taken away (legs 1)
// turns about the axis at radius 4/3: 4 pi / 3. Before, the contact along
// the ring's outer circle was hatched in part only and the rest walked
// along the boundary ("Path failed"), or, for a whole circle, the chamfer
// was valid but kept the ring as a face of zero width. The rim a whole
// circle with the counterbore's seam at its vertex, and in two arcs with
// the seam there or turned away; the whole circle with the seam turned
// away, the p-curves' fit and the fillet need 0144
// (test_step_ring_rim_blends_past_the_seam).
void test_chamfer_as_wide_as_a_step_ring() {
  for (const bool split : {false, true}) {
    for (const bool turned : {false, true}) {
      if (!split && turned) {
        continue; // (patch 0144: test_step_ring_rim_blends_past_the_seam)
      }
      const TopoDS_Shape body = step_ring(split, turned);
      const std::vector<TopoDS_Edge> rim = step_ring_rim(body);
      CHECK(rim.size() == (split ? 2u : 1u));
      const TopoDS_Shape result = chamfered(body, rim, 1.0);
      CHECK(!result.IsNull());
      if (result.IsNull()) {
        continue;
      }
      CHECK(BRepCheck_Analyzer(result).IsValid());
      CHECK_NEAR(volume_of(result) - volume_of(body), -4.0 * kPi / 3.0);
      // The ring gone, the cone in its place (in two pieces on two arcs).
      CHECK(faces_of(result) == faces_of(body) + (split ? 1 : 0));
    }
  }
}

// The step ring's rim of test_chamfer_as_wide_as_a_step_ring chamfered by 1
// in every form, and rounded. With the counterbore's seam turned away from
// the rim's vertex (`turned`), the contact along the ring's outer circle
// passes that circle's vertex, on the seam, inside the stripe (patch 0144):
// its edge was split there with one p-curve on the counterbore's wall for
// both parts, the part past the seam a period off, and the parts' p-curves
// on the chamfer's cone were shifted by the vertex's parameter (an
// unorientable face for the whole circle; for the rim in two arcs a valid
// shape whose p-curves on the cone missed their edges by 2.8). Now the
// stripe is split there in two pieces of one face. The fillet of radius 1
// likewise: it takes the square of the ring's width less the quarter disc
// about the counterbore's rim, turned about the axis: 2 pi (11 / 6 - pi / 2).
void test_step_ring_rim_blends_past_the_seam() {
  for (const bool fillet : {false, true}) {
    for (const bool split : {false, true}) {
      for (const bool turned : {false, true}) {
        const TopoDS_Shape body = step_ring(split, turned);
        const std::vector<TopoDS_Edge> rim = step_ring_rim(body);
        CHECK(rim.size() == (split ? 2u : 1u));
        TopoDS_Shape result;
        if (fillet) {
          BRepFilletAPI_MakeFillet maker(body);
          for (const TopoDS_Edge& edge : rim) {
            if (maker.Contour(edge) == 0) {
              maker.Add(1.0, edge);
            }
          }
          try {
            OCC_CATCH_SIGNALS
            maker.Build();
          } catch (const Standard_Failure& failure) {
            std::fprintf(stderr, "chamfer overflow test: %s\n", failure.what());
          }
          if (maker.IsDone()) {
            result = maker.Shape();
          }
        } else {
          result = chamfered(body, rim, 1.0);
        }
        CHECK(!result.IsNull());
        if (result.IsNull()) {
          continue;
        }
        CHECK(BRepCheck_Analyzer(result).IsValid());
        CHECK(pcurves_fit(result));
        const double change = volume_of(result) - volume_of(body);
        CHECK_NEAR(change, fillet ? -2.0 * kPi * (11.0 / 6.0 - kPi / 2.0) : -4.0 * kPi / 3.0);
        // The ring gone, the blend in its place (in two pieces on two arcs).
        CHECK(faces_of(result) == faces_of(body) + (split ? 1 : 0));
      }
    }
  }
}

// A block with a bore of radius 17 and inside it a lug: a ring sector
// between radii 12.5 and 17 over 135 degrees from `start` degrees, 5 high,
// joined to the bore's wall, whose seam lies at `seam` degrees.
TopoDS_Shape bore_with_a_lug(double start, double seam) {
  const gp_Dir up(0, 0, 1);
  const double a0 = start * kPi / 180.0, s0 = seam * kPi / 180.0;
  const gp_Dir from(std::cos(a0), std::sin(a0), 0);
  TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-30, -30, -16), gp_Pnt(30, 30, 18)).Shape();
  block = cut(block, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -17), up, gp_Dir(std::cos(s0), std::sin(s0), 0)),
                                              17.0, 36)
                         .Shape());
  TopoDS_Shape lug = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -6), up, from), 17.0, 5.0, 0.75 * kPi).Shape();
  lug = cut(lug, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -7), up, from), 12.5, 7.0).Shape());
  return fuse(block, lug);
}

// The concave arc where the lug's top meets the bore's wall above it.
std::vector<TopoDS_Edge> lug_top_arc(const TopoDS_Shape& body) {
  return edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() + 1) < 1e-6 && std::abs(std::hypot(p.X(), p.Y()) - 17.0) < 1e-6;
  });
}

// The lug from 112.5 degrees, its ends away from the bore's seam at 0
// degrees. The concave arc where the lug's top meets the wall above it is
// chamfered by 4.5, as wide as the lug's top (patch 0140): the hatching of
// the circular contact on the top's inner arc gave a domain without a first
// point, and the contact was walked. The chamfer adds the triangle (legs
// 4.5, centroid at radius 15.5) turned through 135 degrees; the lug's
// radial sides cut it. The lug's ends on the seam, the p-curves' fit and
// the fillet need 0147 (test_lug_in_a_bore_blends_on_its_seam).
void test_chamfer_as_wide_as_a_lug_in_a_bore() {
  const TopoDS_Shape body = bore_with_a_lug(112.5, 0.0);
  const std::vector<TopoDS_Edge> arc = lug_top_arc(body);
  CHECK(arc.size() == 1u);
  const TopoDS_Shape result = chamfered(body, arc, 4.5);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK_NEAR(volume_of(result) - volume_of(body), 0.75 * kPi * 15.5 * 10.125);
  // The lug's top gone, the cone in its place.
  CHECK(faces_of(result) == faces_of(body));
}

// The lug's top arc of test_chamfer_as_wide_as_a_lug_in_a_bore chamfered,
// or rounded (`fillet`), by 4.5 with the lug from `start` degrees and the
// bore's seam at `seam` degrees. With the lug's end on the bore's seam
// (patch 0147), the end's corner prolonged the edge between the lug's end
// and the wall up to the chamfer with the edge's p-curve on the wall's
// other side (an edge left from the seam keeps both), and the seam above
// the lug was not cut: a self-intersecting wire. The fillet of radius 4.5
// likewise: it adds the square of the lug's width less the quarter disc
// about the fillet's axis, turned through 135 degrees.
void test_lug_in_a_bore_blends_on_its_seam(double start, double seam, bool fillet) {
  const TopoDS_Shape body = bore_with_a_lug(start, seam);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const std::vector<TopoDS_Edge> arc = lug_top_arc(body);
  CHECK(arc.size() == 1u);
  TopoDS_Shape result;
  if (fillet) {
    BRepFilletAPI_MakeFillet maker(body);
    for (const TopoDS_Edge& edge : arc) {
      maker.Add(4.5, edge);
    }
    try {
      OCC_CATCH_SIGNALS
      maker.Build();
    } catch (const Standard_Failure& failure) {
      std::fprintf(stderr, "chamfer overflow test: %s\n", failure.what());
    }
    if (maker.IsDone()) {
      result = maker.Shape();
    }
  } else {
    result = chamfered(body, arc, 4.5);
  }
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(pcurves_fit(result));
  const double r = 4.5;
  const double added = fillet ? 0.75 * kPi * (r * 66.375 - 12.5 * kPi * r * r / 4.0 - r * r * r / 3.0)
                              : 0.75 * kPi * 15.5 * 10.125;
  CHECK_NEAR(volume_of(result) - volume_of(body), added);
  // The lug's top gone, the blend in its place.
  CHECK(faces_of(result) == faces_of(body));
}

// A wall (a block's face x = 95) with a rib on it: a pentagonal prism over
// (95,85) (94,85) (92,84) (94,83) (95,83) from z = 5 to 50. The rib's
// bottom meets the wall below in a concave edge, chamfered by 2.99: on the
// bottom the contact (x = 92.01) lies inside it only within 0.005 of the
// tip, elsewhere it runs off across the bottom's slanted edges, and the
// rib's sides, prolonged down, cut the chamfer; at the ends of the edge the
// rib's flat sides cut it, the edges between the sides prolonged down to
// it. With the rib's bottom as the chamfer's first face (`bottom_first`:
// the shell's faces in another order), the end's corner prolonged the
// wall's edge on the end face's bounded surface, where the contact's point
// beyond that edge's end was not found (patch 0141).
void test_chamfer_of_a_rib_cut_by_its_sides(bool bottom_first) {
  TopoDS_Shape wall = BRepPrimAPI_MakeBox(gp_Pnt(95, 70, 0), gp_Pnt(105, 100, 60)).Shape();
  BRepBuilderAPI_MakePolygon outline;
  outline.Add(gp_Pnt(95, 85, 5));
  outline.Add(gp_Pnt(94, 85, 5));
  outline.Add(gp_Pnt(92, 84, 5));
  outline.Add(gp_Pnt(94, 83, 5));
  outline.Add(gp_Pnt(95, 83, 5));
  outline.Close();
  const TopoDS_Shape rib =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(outline.Wire()).Face(), gp_Vec(0, 0, 45)).Shape();
  TopoDS_Shape body = fuse(wall, rib);
  if (bottom_first) {
    BRep_Builder builder;
    TopoDS_Shell shell;
    builder.MakeShell(shell);
    std::vector<TopoDS_Shape> rest;
    for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
      BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
      if (surface.GetType() == GeomAbs_Plane && std::abs(surface.Plane().Location().Z() - 5) < 1e-9 &&
          std::abs(surface.Plane().Axis().Direction().Z()) > 0.5) {
        builder.Add(shell, f.Current());
      } else {
        rest.push_back(f.Current());
      }
    }
    for (const TopoDS_Shape& face : rest) {
      builder.Add(shell, face);
    }
    TopoDS_Solid solid;
    builder.MakeSolid(solid);
    builder.Add(solid, shell);
    body = solid;
  }
  const std::vector<TopoDS_Edge> foot = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() - 5) < 1e-6 && std::abs(p.X() - 95) < 1e-6 && p.Y() > 82.9 && p.Y() < 85.1;
  });
  CHECK(foot.size() == 1u);
  const TopoDS_Shape result = chamfered(body, foot, 2.99);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // At depth s below the rib's bottom the chamfer fills the rib's section
  // beyond x = 92.01 + s: the triangle's part (tip at x = 92) while that
  // lies before x = 94, then the rectangle's.
  const double e = 0.01, s1 = 1.99;
  const double added = 4.0 * s1 - (std::pow(e + s1, 3) - std::pow(e, 3)) / 6.0 + 1.0;
  CHECK_NEAR(volume_of(result) - volume_of(body), added);
  // The chamfer's face; the bottom's sliver at the tip (5e-5) stays.
  CHECK(faces_of(result) == faces_of(body) + 1);
}

// A plate 17 thick (z 0..17) whose corner is rounded with radius 20 about
// (81.5, 20), and under it a boss of radius 7.5 on the same axis (z from
// -13, or -14, to 0). `exact`: the boss 13 long; `away`: the boss's seam
// at 45 degrees, else at 180.
TopoDS_Shape boss_under_a_rounded_plate_corner(bool exact, bool away) {
  TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(61.5, 0, 0), gp_Pnt(200, 150, 17)).Shape();
  {
    BRepFilletAPI_MakeFillet round(plate);
    for (const TopoDS_Edge& e : edges_where(plate, [](const gp_Pnt& p) {
           return std::abs(p.X() - 61.5) < 1e-6 && std::abs(p.Y()) < 1e-6;
         })) {
      round.Add(20.0, e);
    }
    plate = round.Shape();
  }
  const double length = exact ? 13.0 : 14.0;
  const TopoDS_Shape boss =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(81.5, 20, -length), gp_Dir(0, 0, 1), away ? gp_Dir(1, 1, 0) : gp_Dir(-1, 0, 0)),
                               7.5, length)
          .Shape();
  return fuse(plate, boss);
}

// The boss's foot, a closed circle on the plate's underside.
std::vector<TopoDS_Edge> boss_foot(const TopoDS_Shape& body) {
  return edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z()) < 1e-6 && std::abs(std::hypot(p.X() - 81.5, p.Y() - 20) - 7.5) < 1e-6;
  });
}

// The volume the boss's foot chamfer of 13 adds: the chamfer's ring (legs
// 13 from radius 7.5) less what lies beyond the outline: in the rounded
// corner's quarter beyond radius 20 (exact), and beyond each side half a
// circular segment of the ring's radius R (20 to 20.5 over the top 0.5),
// integrated with Simpson's rule.
double boss_foot_chamfer_volume() {
  const double ring = 2.0 * kPi * (7.5 + 13.0 / 3.0) * 84.5;
  const double corner = 0.5 * kPi * ((std::pow(20.5, 3) / 6.0 - 200.0 * 20.5) - (8000.0 / 6.0 - 4000.0));
  double sides = 0.0;
  const int n = 2000;
  for (int i = 0; i <= n; ++i) {
    const double r = 20.0 + 0.5 * i / n;
    const double segment = r * r * std::acos(20.0 / r) - 20.0 * std::sqrt(std::max(0.0, r * r - 400.0));
    sides += (i == 0 || i == n ? 1.0 : (i % 2 == 1 ? 4.0 : 2.0)) * segment;
  }
  sides *= 0.5 / n / 3.0;
  return ring - corner - sides;
}

// The boss's foot is chamfered by 13: on the plate's underside the contact
// (radius 20.5) runs 0.5 beyond the plate's outline over a quarter turn and
// more (patch 0142). The chamfer adds material: the plate's sides there,
// prolonged down, cut it, the edges between them prolonged down to it, and
// the underside's edges under the chamfer go. Before, the chain of faces
// beyond was not found (the blend crosses the edges between them below
// their ends), and the gap was left to the walking, which chamfers do not
// do. `exact`: the boss as long as the chamfer (it vanishes, 0140); `away`:
// the boss's seam (and the closed spine's start) away from the gap, else in
// it. The longer boss with its seam in the gap, the p-curves' fit and the
// cone as one face need 0145 and 0146
// (test_boss_foot_chamfer_across_its_seam).
void test_chamfer_of_a_boss_beyond_a_plate_corner(bool exact, bool away) {
  const TopoDS_Shape body = boss_under_a_rounded_plate_corner(exact, away);
  const std::vector<TopoDS_Edge> foot = boss_foot(body);
  CHECK(foot.size() == 1u);
  const TopoDS_Shape result = chamfered(body, foot, 13.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK_NEAR_TOL(exact_volume_of(result) - exact_volume_of(body), boss_foot_chamfer_volume(), 1e-8);
}

// The boss's foot of test_chamfer_of_a_boss_beyond_a_plate_corner chamfered
// by 13 in every form. With the seam in the gap the pieces past the spine's
// start held their p-curves on the boss a period on, beyond its seam (patch
// 0145: an unorientable face for the longer boss); and the cone came in a
// face per piece where the piece across the spine's start holds its
// parameters a period on, or the run of pieces cut by the faces beyond goes
// on across the start (patch 0146), the cone being one face in the file's
// body.
void test_boss_foot_chamfer_across_its_seam(bool exact, bool away) {
  const TopoDS_Shape body = boss_under_a_rounded_plate_corner(exact, away);
  const std::vector<TopoDS_Edge> foot = boss_foot(body);
  CHECK(foot.size() == 1u);
  const TopoDS_Shape result = chamfered(body, foot, 13.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(pcurves_fit(result));
  CHECK_NEAR_TOL(exact_volume_of(result) - exact_volume_of(body), boss_foot_chamfer_volume(), 1e-8);
  // The cone one face; the boss's side gone with the exact length.
  CHECK(faces_of(result) == faces_of(body) + (exact ? 0 : 1));
}

// The plate of test_chamfer_of_a_boss_beyond_a_plate_corner with one side
// slanted: its corner an arc of radius 20 about (81.5, 20) from (81.5, 0),
// where the side along y = 0 leaves it, round to 196.4 degrees, where a
// side tangent to it leaves it towards y = 150 (z from `z0`, `height`
// high). The boss under it (radius 7.5, `length` long) has its axis down,
// so that its foot's circle, the closed spine, runs clockwise from the
// boss's seam at `seam` degrees. Chamfered by 13, the contact on the
// plate's underside (radius 20.5) runs 0.5 beyond the outline from 196.4
// degrees round to 282.7 (patch 0142's gap, cut by the slanted side, the
// corner's cylinder and the side along y = 0). With the seam from 120 to
// 180 degrees the gap is long and ends towards the spine's end: its last
// crossing's parameter on the boss's circle was taken a period early, the
// last piece ran more than once round the circle, and the chamfer was
// invalid (patch 0941; with the boss as long as the chamfer the repair of
// its bottom face then never ended, 0942). The volume: the plate and boss
// with the chamfer's cone cut by the plate's outline prolonged down.
TopoDS_Shape slanted_corner_plate(double z0, double height) {
  const double t = kPi + 0.28618; // where the slanted side leaves the arc
  const gp_Pnt a(81.5, 0, z0);
  const double tm = 0.5 * (1.5 * kPi + t);
  const gp_Pnt m(81.5 + 20 * std::cos(tm), 20 + 20 * std::sin(tm), z0);
  const gp_Pnt b(81.5 + 20 * std::cos(t), 20 + 20 * std::sin(t), z0);
  const double reach = (150.0 - b.Y()) / -std::cos(t);
  const gp_Pnt c(b.X() + reach * std::sin(t), 150.0, z0);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(b, m, a).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(a, gp_Pnt(200, 0, z0)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(200, 0, z0), gp_Pnt(200, 150, z0)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(200, 150, z0), c).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(c, b).Edge());
  return BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire(), true).Face(), gp_Vec(0, 0, height)).Shape();
}

void test_chamfer_of_a_boss_beyond_a_slanted_plate_corner(double seam, double length) {
  const double s = seam * kPi / 180.0;
  const TopoDS_Shape boss =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(81.5, 20, 0), gp_Dir(0, 0, -1), gp_Dir(std::cos(s), std::sin(s), 0)), 7.5,
                               length)
          .Shape();
  const TopoDS_Shape body = fuse(slanted_corner_plate(0, 17), boss);
  const std::vector<TopoDS_Edge> foot = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z()) < 1e-6 && std::abs(std::hypot(p.X() - 81.5, p.Y() - 20) - 7.5) < 1e-6;
  });
  CHECK(foot.size() == 1u);
  const TopoDS_Shape result = chamfered(body, foot, 13.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(pcurves_fit(result));
  const TopoDS_Shape cone =
      BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(81.5, 20, -13), gp_Dir(0, 0, 1)), 7.5, 20.5, 13).Shape();
  BRepAlgoAPI_Common ring(cone, slanted_corner_plate(-length, length));
  const double expected = exact_volume_of(fuse(body, ring.Shape())) - exact_volume_of(body);
  CHECK_NEAR_TOL(exact_volume_of(result) - exact_volume_of(body), expected, 1e-8);
  // The cone one face; the boss's side gone with the exact length.
  CHECK(faces_of(result) == faces_of(body) + (length == 13.0 ? 0 : 1));
}

// A hexagonal prism (inradius 4, z 0..4) under a disc of radius 8.5 (z
// 4..6); the edges where the prism's walls meet the disc's underside are
// chamfered 4 on the underside and 3 on the walls (patch 0143). At the
// hexagon's corners the two chamfers' contacts on the underside cross 9.24
// from the axis, beyond the disc's rim: the disc's cylinder, prolonged
// down, cuts the chamfers there, and their intersection runs from the
// prism's edge to the cylinder (at height 3.52). Before, the corner by
// intersection on the common face found no crossing and failed.
// `seam_at_corner`: the cylinder's seam on the hexagon's corner at 180
// degrees (the corner's point on the seam's prolongation), else at 30.
void test_chamfers_crossing_beyond_their_common_face(bool seam_at_corner) {
  BRepBuilderAPI_MakePolygon outline;
  const double r = 8.0 / std::sqrt(3.0);
  for (int k = 0; k < 6; ++k) {
    outline.Add(gp_Pnt(r * std::cos(k * kPi / 3.0), r * std::sin(k * kPi / 3.0), 0));
  }
  outline.Close();
  const TopoDS_Shape prism =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(outline.Wire()).Face(), gp_Vec(0, 0, 4)).Shape();
  const gp_Dir seam = seam_at_corner ? gp_Dir(-1, 0, 0) : gp_Dir(std::sqrt(3.0), 1, 0);
  const TopoDS_Shape disc = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 4), gp_Dir(0, 0, 1), seam), 8.5, 2).Shape();
  const TopoDS_Shape body = fuse(prism, disc);
  const std::vector<TopoDS_Edge> feet = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() - 4) < 1e-6 && std::hypot(p.X(), p.Y()) < 4.62;
  });
  CHECK(feet.size() == 6u);
  TopoDS_Face underside;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    if (surface.GetType() == GeomAbs_Plane && std::abs(surface.Plane().Location().Z() - 4) < 1e-9 &&
        std::abs(surface.Plane().Axis().Direction().Z()) > 0.5) {
      underside = TopoDS::Face(f.Current());
    }
  }
  BRepFilletAPI_MakeChamfer maker(body);
  for (const TopoDS_Edge& edge : feet) {
    maker.Add(4.0, 3.0, edge, underside);
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "chamfer overflow test: %s\n", failure.what());
  }
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    return;
  }
  const TopoDS_Shape result = maker.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // At height z (1 to 4) the chamfers fill the hexagon of inradius 4 + s,
  // s = (z - 1) 4 / 3, within the disc's cylinder, less the prism's: the
  // hexagon of inradius a within radius R is twelve times the triangle up
  // to where its side leaves the circle and the sector beyond.
  const auto hexagon_in_disc = [](double a, double R) {
    const double t = std::acos(std::min(1.0, a / R));
    if (t >= kPi / 6.0) {
      return 2.0 * std::sqrt(3.0) * a * a;
    }
    return 12.0 * (0.5 * a * a * std::tan(t) + 0.5 * R * R * (kPi / 6.0 - t));
  };
  const int n = 20000;
  double added = 0.0;
  for (int i = 0; i <= n; ++i) {
    const double s = 4.0 * i / n;
    const double area = hexagon_in_disc(4.0 + s, 8.5) - 2.0 * std::sqrt(3.0) * 16.0;
    added += (i == 0 || i == n ? 1.0 : (i % 2 == 1 ? 4.0 : 2.0)) * area;
  }
  added *= 3.0 / n / 3.0;
  CHECK_NEAR_TOL(volume_of(result) - volume_of(body), added, 1e-8);
  // Six chamfers, the underside in six pieces between them and the rim.
  CHECK(faces_of(result) == faces_of(body) + 11);
}

// A floor (z 0..3) with a ledge `ledge_height` high and on it a wall 20
// high; the wall's outline is a line 30 long along y = 37.5 and a quarter
// arc of radius 20 about (57.5, 57.5) meeting it tangentially, the ledge's
// the same 2 further out. The wall's foot on the ledge (the line and the
// arc) is chamfered by 2, as wide as the ledge (patch 0148): the ledge's top
// vanishes and the chamfer's contact runs along its outer edges. Before,
// the arc's contact, running on into the ledge's top where the line's
// begins, was hatched to a point; the ledge's top, along both edges, was
// not taken as consumed; and the line's and the arc's pieces ended at two
// distinct vertices where they meet: the chamfer failed, or its wires
// intersected. It adds the triangle (legs 2) along the line and, about the
// arc's axis, at radius 20 + 2 / 3 over a quarter turn.
void test_chamfer_as_wide_as_a_ledge_along_a_line_and_an_arc(double ledge_height) {
  const auto outline = [](double offset, double z, double height) {
    const gp_Pnt a(87.5, 37.5 - offset, z), b(57.5, 37.5 - offset, z), c(37.5 - offset, 57.5, z), e(87.5, 57.5, z);
    BRepBuilderAPI_MakeWire wire;
    wire.Add(BRepBuilderAPI_MakeEdge(a, b).Edge());
    wire.Add(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(57.5, 57.5, z), gp_Dir(0, 0, 1)), 20.0 + offset), c, b)
                 .Edge());
    wire.Add(BRepBuilderAPI_MakeEdge(c, e).Edge());
    wire.Add(BRepBuilderAPI_MakeEdge(e, a).Edge());
    return BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire()).Face(), gp_Vec(0, 0, height)).Shape();
  };
  const TopoDS_Shape floor = BRepPrimAPI_MakeBox(gp_Pnt(20, 20, 0), gp_Pnt(100, 70, 3)).Shape();
  const TopoDS_Shape body =
      fuse(fuse(floor, outline(2.0, 3.0, ledge_height)), outline(0.0, 3.0 + ledge_height, 20.0));
  const double z = 3.0 + ledge_height;
  const std::vector<TopoDS_Edge> foot = edges_where(body, [z](const gp_Pnt& p) {
    return std::abs(p.Z() - z) < 1e-6 && ((std::abs(p.Y() - 37.5) < 1e-6 && p.X() > 57.4) ||
                                          std::abs(std::hypot(p.X() - 57.5, p.Y() - 57.5) - 20.0) < 1e-6);
  });
  CHECK(foot.size() == 2u);
  const TopoDS_Shape result = chamfered(body, foot, 2.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(pcurves_fit(result));
  const double d = 2.0;
  CHECK_NEAR_TOL(exact_volume_of(result) - exact_volume_of(body),
                 d * d / 2.0 * 30.0 + d * d / 2.0 * kPi / 2.0 * (20.0 + d / 3.0), 1e-8);
  // The ledge's top gone, the chamfer's plane and cone in its place.
  CHECK(faces_of(result) == faces_of(body) + 1);
}

// A plate (z 0..10) with under it a boss of radius 12.5, 3 long, its seam
// at `seam` degrees; the boss's free end's edge is chamfered by 3, as long
// as the boss (or by distance 3 and 45 degrees on the end, `by_angle`): the
// boss's wall vanishes and the chamfer's contact runs along the closed edge
// where the boss meets the plate (patch 0149). Before, the contact's end
// snapped onto that edge's vertex was projected onto the line's other end
// (the vertex's parameters a period away on the boss's wall), and the
// chamfer failed. It takes the triangle (legs 3) turned about the axis at
// radius 12.5 - 1.
void test_chamfer_as_long_as_a_boss(double seam, bool by_angle) {
  const double s0 = seam * kPi / 180.0;
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-30, -30, 0), gp_Pnt(30, 30, 10)).Shape();
  const TopoDS_Shape body =
      fuse(plate, BRepPrimAPI_MakeCylinder(
                      gp_Ax2(gp_Pnt(0, 0, -3), gp_Dir(0, 0, 1), gp_Dir(std::cos(s0), std::sin(s0), 0)), 12.5, 3)
                      .Shape());
  const std::vector<TopoDS_Edge> rim = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() + 3) < 1e-6 && std::abs(std::hypot(p.X(), p.Y()) - 12.5) < 1e-6;
  });
  CHECK(rim.size() == 1u);
  if (rim.size() != 1u) {
    return;
  }
  TopoDS_Shape result;
  if (by_angle) {
    TopoDS_Face end;
    for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
      BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
      if (surface.GetType() == GeomAbs_Plane && std::abs(surface.Plane().Location().Z() + 3) < 1e-9) {
        end = TopoDS::Face(f.Current());
      }
    }
    BRepFilletAPI_MakeChamfer maker(body);
    maker.AddDA(3.0, kPi / 4.0, rim[0], end);
    try {
      OCC_CATCH_SIGNALS
      maker.Build();
    } catch (const Standard_Failure& failure) {
      std::fprintf(stderr, "chamfer overflow test: %s\n", failure.what());
    }
    if (maker.IsDone()) {
      result = maker.Shape();
    }
  } else {
    result = chamfered(body, rim, 3.0);
  }
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(pcurves_fit(result));
  CHECK_NEAR_TOL(exact_volume_of(result) - exact_volume_of(body), -2.0 * kPi * 11.5 * 4.5, 1e-8);
  // The boss's wall gone, the chamfer's cone in its place.
  CHECK(faces_of(result) == faces_of(body));
}

// A prism of the rounded rectangle [x0, x1] x [y0, y1] (corner radius r)
// from z0 up by h.
TopoDS_Shape rounded_prism(double x0, double y0, double x1, double y1, double r, double z0, double h) {
  BRepBuilderAPI_MakeWire wire;
  const auto line = [&](double ax, double ay, double bx, double by) {
    wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(ax, ay, z0), gp_Pnt(bx, by, z0)).Edge());
  };
  const auto arc = [&](double cx, double cy, double ax, double ay, double bx, double by) {
    wire.Add(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(cx, cy, z0), gp_Dir(0, 0, 1)), r), gp_Pnt(ax, ay, z0),
                                     gp_Pnt(bx, by, z0))
                 .Edge());
  };
  line(x0 + r, y0, x1 - r, y0);
  arc(x1 - r, y0 + r, x1 - r, y0, x1, y0 + r);
  line(x1, y0 + r, x1, y1 - r);
  arc(x1 - r, y1 - r, x1, y1 - r, x1 - r, y1);
  line(x1 - r, y1, x0 + r, y1);
  arc(x0 + r, y1 - r, x0 + r, y1, x0, y1 - r);
  line(x0, y1 - r, x0, y0 + r);
  arc(x0 + r, y0 + r, x0, y0 + r, x0 + r, y0);
  return BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire()).Face(), gp_Vec(0, 0, h)).Shape();
}

// a cut b, its faces of one surface merged (the pocket's floor in one face).
TopoDS_Shape cut_simplified(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepAlgoAPI_Cut op(a, b);
  op.SimplifyResult();
  return op.Shape();
}

// OCCT's fillet of the edges with one radius: the result, or a null shape
// when it fails.
TopoDS_Shape filleted(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges, double radius) {
  BRepFilletAPI_MakeFillet maker(body);
  for (const TopoDS_Edge& edge : edges) {
    if (maker.Contour(edge) == 0) {
      maker.Add(radius, edge);
    }
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "chamfer overflow test: %s\n", failure.what());
    return TopoDS_Shape();
  }
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// A block (top z = 12) with a pocket 60 x 40 of corner radius 7.5, in two
// steps: a wall `wall` high stands on a ledge `ledge` wide round the deeper
// part (down to z = 3). The wall's foot, a closed chain of four lines and
// four arcs, is chamfered (or rounded) by 1.5. With `lip` the steps are the
// other way round: the upper opening is the narrower one (inset by the
// ledge, corner radius 7.5 - ledge), a lip `wall` high over the wider part,
// and its edge with the lip's underside (convex) is chamfered. A wall and a
// ledge of 1.5 both vanish (patches 0351, 0352). Before, along the chain's
// lines the contact on the ledge, on its boundary, was hatched into the
// pieces beyond the lines' ends only (where the ledge's edge turns away),
// so that the stripe was walked there and failed, a chamfer's walking not
// being implemented ("Path failed"); with the contacts along the edges the
// stripe's point at its start was not the vertex its ends were made, and
// the topological build failed (TopoDS_Builder::Add); and on the lip the
// contact along the top's edge gave the top a transition into itself, the
// chamfer folding back under the top: an invalid shape, 18 of 24 faces. It
// adds (removes, on the lip) the triangle (legs d) along the lines and,
// about the arcs' axes, at radius 7.5 - d / 3 (on the lip 6 + d / 3; for
// a rounding the corner left by the ball, at its centroid's radius).
void test_wall_foot_round_a_stepped_pocket(double wall, double ledge, bool lip, bool fillet) {
  const double T = 12.0, X = 60.0, Y = 40.0, r = 7.5, d = 1.5;
  const double zl = T - wall;
  TopoDS_Shape body = BRepPrimAPI_MakeBox(gp_Pnt(-1.5, -1.5, 0), gp_Pnt(X + 1.5, Y + 1.5, T)).Shape();
  if (lip) {
    body = cut_simplified(body, rounded_prism(ledge, ledge, X - ledge, Y - ledge, r - ledge, zl, wall + 1));
    body = cut_simplified(body, rounded_prism(0, 0, X, Y, r, 3.0, zl - 3.0));
  } else {
    body = cut_simplified(body, rounded_prism(0, 0, X, Y, r, zl, wall + 1));
    body = cut_simplified(body, rounded_prism(ledge, ledge, X - ledge, Y - ledge, r - ledge, 3.0, T));
  }
  // The chamfered outline: inset o, corner radius ro.
  const double o = lip ? ledge : 0.0, ro = r - o;
  const std::vector<TopoDS_Edge> foot = edges_where(body, [&](const gp_Pnt& p) {
    if (std::abs(p.Z() - zl) > 1e-7) {
      return false;
    }
    const double dx = std::max({0.0, o + ro - p.X(), p.X() - (X - o - ro)});
    const double dy = std::max({0.0, o + ro - p.Y(), p.Y() - (Y - o - ro)});
    if (dx > 0 && dy > 0) {
      return std::abs(std::hypot(dx, dy) - ro) < 1e-6;
    }
    return (dx == 0 && (std::abs(p.Y() - o) < 1e-6 || std::abs(p.Y() - (Y - o)) < 1e-6)) ||
           (dy == 0 && (std::abs(p.X() - o) < 1e-6 || std::abs(p.X() - (X - o)) < 1e-6));
  });
  CHECK(foot.size() == 8u);
  const TopoDS_Shape result = fillet ? filleted(body, foot, d) : chamfered(body, foot, d);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    std::fprintf(stderr, "stepped pocket: wall %g ledge %g lip %d rounded %d failed\n", wall, ledge, lip ? 1 : 0,
                 fillet ? 1 : 0);
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(pcurves_fit(result));
  const double lines = 2.0 * (X - 2.0 * r) + 2.0 * (Y - 2.0 * r);
  // The cross-section's area and its centroid's distance from the edge.
  const double area = fillet ? (1.0 - kPi / 4.0) * d * d : d * d / 2.0;
  const double off = fillet ? d * (10.0 - 3.0 * kPi) / (3.0 * (4.0 - kPi)) : d / 3.0;
  const double expected = lip ? -area * (lines + 2.0 * kPi * (ro + off)) : area * (lines + 2.0 * kPi * (ro - off));
  CHECK_NEAR_TOL(exact_volume_of(result) - exact_volume_of(body), expected, 1e-8);
  // The wall's eight faces and the ledge vanish where as high or as wide as
  // the chamfer; its eight faces come in.
  const int vanished = (wall <= d + 1e-9 ? 8 : 0) + (ledge <= d + 1e-9 ? 1 : 0);
  CHECK(faces_of(result) == faces_of(body) - vanished + 8);
}

// A plate `height` high whose outline turns at a crease from a straight
// side into an arc, the rim rounded with radius 1 along both: a ring about
// the vertical axis through (0, -5) between radii 10 and 20, its outer top
// and bottom edges rounded, kept where x, y >= 0, and its section at y = 0
// extruded 10 along -y. The crease (the straight side's wall x = sqrt(375)
// meets the ring's cylinder) runs up the wall and on over both roundings to
// the top and the bottom, where its edge meets the rims' edges, along which
// the roundings join the top (and the bottom) tangentially. The chain is
// chamfered by `distance`.
//
// Patch 0380: the walk reaches the chain's end on the top with its contact
// on the straight side's rounding at that rounding's edge and its other
// contact inside the ring's torus; the corner by intersection with the top,
// tangent to both, found nothing, and the general corner threw in its
// projection (the chamfer failed). Now a ruled piece closes the end, from
// the end section to the point of the ring's rim as far from the vertex as
// the section's point on the straight rim, along a line on the top and a
// line of the torus's parameters; the top loses the triangle of the
// straight rim, the chord and the vertex with the arc's segment.
//
// Patch 0381: the chain's guide, made C2 by removing its knots within a
// tenth of its length, ran 0.03 off the crease along the wall and moved the
// chamfer's contacts there; now a chamfer's guide is deformed by a
// hundredth of its distance at most, and the wall's section of the chamfer
// is the triangle of the crease, the contact on the straight wall and the
// one on the cylinder, with the cylinder's segment.
//
// The roundings' parts are walked; their volume does not depend on the
// height, so the plates `height` and `height` + 10 high lose volumes that
// differ by the wall's section times 10.
struct RoundedCrease {
  TopoDS_Shape body;
  TopoDS_Shape result;
};

RoundedCrease rounded_crease(double height, double distance) {
  const double r = 1.0, ri = 10.0, ro = 20.0, c = -5.0;
  const gp_Dir ny(0, -1, 0);
  const auto at = [c](double rho, double z) { return gp_Pnt(rho, c, z); };
  BRepBuilderAPI_MakeWire meridian;
  meridian.Add(BRepBuilderAPI_MakeEdge(at(ri, 0), at(ro - r, 0)).Edge());
  meridian.Add(
      BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(at(ro - r, r), ny, gp_Dir(1, 0, 0)), r), at(ro - r, 0), at(ro, r)).Edge());
  meridian.Add(BRepBuilderAPI_MakeEdge(at(ro, r), at(ro, height - r)).Edge());
  meridian.Add(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(at(ro - r, height - r), ny, gp_Dir(1, 0, 0)), r),
                                       at(ro, height - r), at(ro - r, height))
                   .Edge());
  meridian.Add(BRepBuilderAPI_MakeEdge(at(ro - r, height), at(ri, height)).Edge());
  meridian.Add(BRepBuilderAPI_MakeEdge(at(ri, height), at(ri, 0)).Edge());
  const TopoDS_Face section = BRepBuilderAPI_MakeFace(meridian.Wire(), true).Face();
  const gp_Pnt axis(0, c, 0);
  const TopoDS_Shape ring = fuse(BRepPrimAPI_MakeRevol(section, gp_Ax1(axis, gp_Dir(0, 0, 1)), kPi / 3).Shape(),
                                 BRepPrimAPI_MakeRevol(section, gp_Ax1(axis, gp_Dir(0, 0, -1)), kPi / 3).Shape());
  const TopoDS_Shape part =
      BRepAlgoAPI_Common(ring, BRepPrimAPI_MakeBox(gp_Pnt(0, 0, -1), gp_Pnt(40, 40, height + 1)).Shape()).Shape();
  TopoDS_Face cut;
  for (TopExp_Explorer f(part, TopAbs_FACE); f.More(); f.Next()) {
    BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    if (surface.GetType() == GeomAbs_Plane && std::abs(surface.Plane().Axis().Direction().Y()) > 0.99) {
      GProp_GProps props;
      BRepGProp::SurfaceProperties(f.Current(), props);
      if (std::abs(props.CentreOfMass().Y()) < 1e-9) {
        cut = TopoDS::Face(f.Current());
      }
    }
  }
  RoundedCrease crease;
  CHECK(!cut.IsNull());
  if (cut.IsNull()) {
    return crease;
  }
  crease.body = fuse(part, BRepPrimAPI_MakePrism(cut, gp_Vec(0, -10, 0)).Shape());
  CHECK(BRepCheck_Analyzer(crease.body).IsValid());
  const double x0 = std::sqrt((ro - r) * (ro - r) - c * c);
  const std::vector<TopoDS_Edge> chain =
      edges_where(crease.body, [x0](const gp_Pnt& p) { return std::abs(p.Y()) < 1e-7 && p.X() > x0 - 1e-6; });
  CHECK(chain.size() == 3u);
  crease.result = chamfered(crease.body, chain, distance);
  return crease;
}

void test_chamfer_of_a_rounded_crease(double height, double distance) {
  const double ro = 20.0, c = -5.0;
  // The wall's section: the crease C, the contact C - d y on the straight
  // wall, the one on the cylinder at distance d from C (an angle phi on),
  // and the cylinder's segment over the chord to it.
  const double xc = std::sqrt(ro * ro - c * c), phi = 2.0 * std::asin(distance / (2.0 * ro));
  const double xb = ro * std::cos(std::atan2(-c, xc) + phi);
  const double wall = 0.5 * distance * std::abs(xb - xc) + 0.5 * ro * ro * (phi - std::sin(phi));
  double lost[2] = {0.0, 0.0};
  for (int k = 0; k < 2; ++k) {
    const double h = height + 10.0 * k;
    const RoundedCrease crease = rounded_crease(h, distance);
    CHECK(!crease.result.IsNull());
    if (crease.result.IsNull()) {
      return;
    }
    CHECK(BRepCheck_Analyzer(crease.result).IsValid());
    CHECK(pcurves_fit(crease.result));
    // The chamfer's faces along the chain and its two end pieces.
    CHECK(faces_of(crease.result) == faces_of(crease.body) + 5);
    lost[k] = volume_of(crease.body) - volume_of(crease.result);
    // A slab across the wall: the section exactly.
    const TopoDS_Shape slab = BRepPrimAPI_MakeBox(gp_Pnt(xc - 5, -3, h / 2 - 0.5), gp_Pnt(xc + 1, 3, h / 2 + 0.5)).Shape();
    CHECK_NEAR_TOL(volume_of(BRepAlgoAPI_Common(crease.body, slab).Shape()) -
                       volume_of(BRepAlgoAPI_Common(crease.result, slab).Shape()),
                   wall, 1e-7);
    // The top: the triangle of the vertex V on the rims, the end's point
    // V - d y on the straight rim and Q on the ring's rim (radius 19) at
    // distance d from V, with the rim's segment over V Q.
    const auto top = [h](const TopoDS_Shape& shape) {
      double area = 0.0;
      for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
        BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
        if (surface.GetType() == GeomAbs_Plane && std::abs(surface.Plane().Axis().Direction().Z()) > 0.99 &&
            std::abs(surface.Plane().Location().Z() - h) < 1e-9) {
          GProp_GProps props;
          BRepGProp::SurfaceProperties(f.Current(), props);
          area += props.Mass();
        }
      }
      return area;
    };
    const double rv = ro - 1.0, xv = std::sqrt(rv * rv - c * c), pv = 2.0 * std::asin(distance / (2.0 * rv));
    const double xq = rv * std::cos(std::atan2(-c, xv) + pv);
    CHECK_NEAR_TOL(top(crease.body) - top(crease.result),
                   0.5 * distance * std::abs(xq - xv) + 0.5 * rv * rv * (pv - std::sin(pv)), 1e-9);
  }
  CHECK_NEAR_TOL(lost[1] - lost[0], 10.0 * wall, 1e-5);
}

// A wall of a ring sector between radii 112 and 115 over 36 degrees,
// standing on a flange 3 thick out to 116.5 whose rim is rounded all round
// (a half circle of radius 1.5); with `rounding` the edge where the rim
// meets the flange's underside (tangentially) is rounded as well, which
// puts a blend face between them.
// The wall's foot on the flange (r 115) is chamfered by 1.5, as wide as the
// flange's top (mitcad#327): the top vanishes and the chamfer's contact runs
// along the top's edge with the rim, whose rounding goes on beyond it
// tangentially. Before patch 0352, the rim took the transition into
// itself: an invalid shape (with the underside rounded, also 6 mm3 too
// much). It adds the triangle (legs 1.5) turned about the axis at radius
// 115 + 0.5.
void test_chamfer_as_wide_as_a_flange_with_a_rounded_rim(double rounding) {
  const double angle = 0.2 * kPi;
  const auto at = [](double r, double z) { return gp_Pnt(r, 0, z); };
  BRepBuilderAPI_MakeWire outline;
  outline.Add(BRepBuilderAPI_MakeEdge(at(112, 2.5), at(115, 2.5)).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(at(115, 2.5), at(115, -34.5)).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(at(115, -34.5), at(116.5, -34.5)).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(at(116.5, -36), gp_Dir(0, -1, 0), gp_Dir(1, 0, 0)), 1.5),
                                      at(116.5, -34.5), at(116.5, -37.5))
                  .Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(at(116.5, -37.5), at(112, -37.5)).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(at(112, -37.5), at(112, 2.5)).Edge());
  const gp_Ax1 axis(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  gp_Trsf turn;
  turn.SetRotation(axis, 0.3 * kPi);
  TopoDS_Shape body = BRepPrimAPI_MakeRevol(BRepBuilderAPI_MakeFace(outline.Wire(), true).Face(), axis, angle)
                          .Shape()
                          .Moved(TopLoc_Location(turn));
  if (rounding > 0.0) {
    const std::vector<TopoDS_Edge> under = edges_where(body, [](const gp_Pnt& p) {
      return std::abs(p.Z() + 37.5) < 1e-6 && std::abs(std::hypot(p.X(), p.Y()) - 116.5) < 1e-6;
    });
    CHECK(under.size() == 1u);
    body = filleted(body, under, rounding);
    CHECK(!body.IsNull());
    if (body.IsNull()) {
      return;
    }
  }
  const std::vector<TopoDS_Edge> foot = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() + 34.5) < 1e-6 && std::abs(std::hypot(p.X(), p.Y()) - 115.0) < 1e-6;
  });
  CHECK(foot.size() == 1u);
  const TopoDS_Shape result = chamfered(body, foot, 1.5);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(pcurves_fit(result));
  CHECK_NEAR_TOL(exact_volume_of(result) - exact_volume_of(body), 1.125 * 115.5 * angle, 1e-8);
  // The flange's top gone, the chamfer's cone in its place.
  CHECK(faces_of(result) == faces_of(body));
}

// A slab (z -2..0) and on it a boss 2 narrower on every side (z 0..4), the
// boss's corners at y = 35 rounded with 3, the slab's with 5. The boss's foot
// along x = 0, the two arcs, y = 35 and x = 28 is chamfered (or rounded) by
// 2, as wide as the slab's top round the boss (patch 0440, mitcad#334): the
// top vanishes there and stays along y = 0, where the chain ends. With
// `split` the top's far edge along y = 37 is split at x = 14, and with
// `split_side` also the one along x = -2 at y = 10 (the contact along it
// starting inside the first part). Before, the contacts on the top along the
// sides ended inside the far edges' first parts with points of their own
// beside the vertices where the arcs' contacts began (the wires of the
// slab's walls not closed), and a contact along two edges of the top end to
// end was along neither (the top kept a strip of no width there): invalid
// shapes. It adds the triangle (legs 2) or the corner left by the ball
// (radius 2) along the lines and about the arcs' axes at the centroid's
// radius.
void test_chamfer_consuming_a_strip_along_a_chain_of_edges(bool split, bool split_side, bool fillet) {
  const auto boss_on_slab = [](double x0, double y0, double x1, double y1, double r, double z0, double h) {
    BRepBuilderAPI_MakeWire wire;
    const auto line = [&](double ax, double ay, double bx, double by) {
      wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(ax, ay, z0), gp_Pnt(bx, by, z0)).Edge());
    };
    const auto arc = [&](double cx, double cy, double ax, double ay, double bx, double by) {
      wire.Add(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(cx, cy, z0), gp_Dir(0, 0, 1)), r), gp_Pnt(ax, ay, z0),
                                       gp_Pnt(bx, by, z0))
                   .Edge());
    };
    line(x0, y0, x1, y0);
    line(x1, y0, x1, y1 - r);
    arc(x1 - r, y1 - r, x1, y1 - r, x1 - r, y1);
    line(x1 - r, y1, x0 + r, y1);
    arc(x0 + r, y1 - r, x0 + r, y1, x0, y1 - r);
    line(x0, y1 - r, x0, y0);
    return BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire()).Face(), gp_Vec(0, 0, h)).Shape();
  };
  TopoDS_Shape body = BRepAlgoAPI_Fuse(boss_on_slab(-2, -2, 30, 37, 5, -2, 2), boss_on_slab(0, 0, 28, 35, 3, 0, 4)).Shape();
  if (split) {
    BRepAlgoAPI_Splitter splitter;
    NCollection_List<TopoDS_Shape> arguments;
    NCollection_List<TopoDS_Shape> tools;
    arguments.Append(body);
    tools.Append(BRepBuilderAPI_MakeVertex(gp_Pnt(14, 37, 0)).Vertex());
    if (split_side) {
      tools.Append(BRepBuilderAPI_MakeVertex(gp_Pnt(-2, 10, 0)).Vertex());
    }
    splitter.SetArguments(arguments);
    splitter.SetTools(tools);
    splitter.Build();
    body = TopExp_Explorer(splitter.Shape(), TopAbs_SOLID).Current();
  }
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const std::vector<TopoDS_Edge> foot = edges_where(body, [](const gp_Pnt& p) {
    if (std::abs(p.Z()) > 1e-6 || p.Y() < -1e-6) {
      return false;
    }
    if (std::abs(p.X()) < 1e-6 || std::abs(p.X() - 28) < 1e-6 || std::abs(p.Y() - 35) < 1e-6) {
      return true;
    }
    return p.Y() > 32 - 1e-6 && std::abs(std::hypot(p.X() - (p.X() < 14 ? 3 : 25), p.Y() - 32) - 3) < 1e-6;
  });
  CHECK(foot.size() == 5u);
  const double d = 2.0;
  const TopoDS_Shape result = fillet ? filleted(body, foot, d) : chamfered(body, foot, d);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(pcurves_fit(result));
  const double area = fillet ? (1.0 - kPi / 4.0) * d * d : d * d / 2.0;
  const double off = fillet ? d * (10.0 - 3.0 * kPi) / (3.0 * (4.0 - kPi)) : d / 3.0;
  CHECK_NEAR_TOL(exact_volume_of(result) - exact_volume_of(body), area * (2 * 32.0 + 22.0 + kPi * (3.0 + off)), 1e-8);
  // The top along y = 0 and the blend's three planes (or cylinders) and two
  // cones (or tori).
  CHECK(faces_of(result) == faces_of(body) + 5);
}

} // namespace

// A post of radius 10 (z 10..30) on a square plate 30 wide (z 0..10); the
// post's foot chamfered 6, so that the cone's contact on the plate (radius
// 16) runs past the plate's sides around their middles, and the sides grow
// up to the cone there (0142). The post's seam at `seam` degrees: inside a
// side's gap (0, 10, 90) or where the contact lies on the plate (45). The
// cone's piece over a gap holding the seam, the spine's start, ran across
// it (patch 0481): the post's cylinder was kept whole, an invalid shape.
// With the plate turned by `turn` radians (1e-8), the sides parallel to the
// seam's direction are so nearly parallel to it that the cone meets them in
// hyperbolas crossing the seam far away twice, and the surfaces'
// intersection kept only a piece of no length between those crossings
// (patch 0480): the chamfer failed. The ring of the chamfer (legs 6 from
// radius 10) less the parts beyond the four sides, each the integral over
// the cone's radius R from 15 to 16 of the circular segment beyond the side.
void test_chamfer_of_a_post_beyond_a_square_plate(double seam, double turn) {
  TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-15, -15, 0), gp_Pnt(15, 15, 10)).Shape();
  if (turn != 0.0) {
    gp_Trsf rotation;
    rotation.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), turn);
    plate = BRepBuilderAPI_Transform(plate, rotation, true).Shape();
  }
  const double a = seam * kPi / 180.0;
  const TopoDS_Shape post =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 10), gp_Dir(0, 0, 1), gp_Dir(std::cos(a), std::sin(a), 0)), 10.0, 20.0)
          .Shape();
  const TopoDS_Shape body = fuse(plate, post);
  const std::vector<TopoDS_Edge> foot = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() - 10.0) < 1e-6 && std::abs(std::hypot(p.X(), p.Y()) - 10.0) < 1e-6;
  });
  CHECK(foot.size() == 1u);
  const TopoDS_Shape result = chamfered(body, foot, 6.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(pcurves_fit(result));
  const double ring = kPi * 36.0 * (10.0 + 6.0 / 3.0);
  double beyond = 0.0;
  const int n = 2000;
  for (int i = 0; i <= n; ++i) {
    const double r = 15.0 + 1.0 * i / n;
    const double segment = r * r * std::acos(15.0 / r) - 15.0 * std::sqrt(std::max(0.0, r * r - 225.0));
    beyond += (i == 0 || i == n ? 1.0 : (i % 2 == 1 ? 4.0 : 2.0)) * segment;
  }
  beyond *= 4.0 / n / 3.0;
  CHECK_NEAR_TOL(exact_volume_of(result) - exact_volume_of(body), ring - beyond, 1e-8);
}

// A fin 1 thick (x in [-0.5, 0.5]) on a plate, its tip a cubic B-spline
// profile (y in [-4, 4], 3.64 high at the ends, flat there, dipping to
// `dip` in the middle) extruded along x, the foot of its side x = 0.5
// chamfered 2.6 up the side and 2 along the plate. The strip of the side
// between that chamfer and the tip is 1.04 high at the ends and narrower in
// the middle than the tip's chamfer of 0.6 (`by_angle`: 0.6 on the side at
// 45 degrees) along the side's tip edge (patch 0531). The material the
// chamfer takes away is its section's triangle less the part inside the
// foot's chamfer extended (z <= 2.6 - 1.3 (x - 0.5)): where the strip is
// wider, the chamfer as usual; in the middle no chamfer face, the foot's
// chamfer running on up to the tip; between them the chamfer cut by the
// foot's plane across its width. Its volume: the clipped triangles of the
// sections square to the edge, swept along it with the edge's curvature
// (Pappus). Before, the walk reached the strip's far edge and would roll on
// it, which chamfers do not ("PerformSurf Not Implemented").
void test_chamfer_wider_than_a_fin_side_strip(double dip, bool by_angle) {
  const double high = 3.64;
  NCollection_Array1<gp_Pnt> poles(1, 6);
  poles(1) = gp_Pnt(-0.5, -4, high);
  poles(2) = gp_Pnt(-0.5, -2.5, high);
  poles(3) = gp_Pnt(-0.5, -1, dip);
  poles(4) = gp_Pnt(-0.5, 1, dip);
  poles(5) = gp_Pnt(-0.5, 2.5, high);
  poles(6) = gp_Pnt(-0.5, 4, high);
  NCollection_Array1<double> knots(1, 4);
  NCollection_Array1<int> mults(1, 4);
  for (int i = 1; i <= 4; ++i) {
    knots(i) = i - 1;
    mults(i) = i == 1 || i == 4 ? 4 : 1;
  }
  const occ::handle<Geom_BSplineCurve> tip = new Geom_BSplineCurve(poles, knots, mults, 3);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-0.5, -4, -1), gp_Pnt(-0.5, 4, -1)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-0.5, 4, -1), gp_Pnt(-0.5, 4, high)).Edge());
  wire.Add(TopoDS::Edge(BRepBuilderAPI_MakeEdge(tip).Edge().Reversed()));
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-0.5, -4, high), gp_Pnt(-0.5, -4, -1)).Edge());
  const TopoDS_Shape fin =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire(), true).Face(), gp_Vec(1, 0, 0)).Shape();
  TopoDS_Shape body = fuse(BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, -5), gp_Pnt(10, 10, 0)).Shape(), fin);
  TopoDS_Face side;
  for (TopExp_Explorer ex(body, TopAbs_FACE); ex.More(); ex.Next()) {
    BRepAdaptor_Surface surface(TopoDS::Face(ex.Current()));
    if (surface.GetType() == GeomAbs_Plane && std::abs(std::abs(surface.Plane().Axis().Direction().X()) - 1) < 1e-9 &&
        std::abs(surface.Plane().Location().X() - 0.5) < 1e-9) {
      side = TopoDS::Face(ex.Current());
    }
  }
  CHECK(!side.IsNull());
  const std::vector<TopoDS_Edge> foot = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.X() - 0.5) < 1e-6 && std::abs(p.Z()) < 1e-6;
  });
  CHECK(foot.size() == 1u);
  if (side.IsNull() || foot.size() != 1u) {
    return;
  }
  BRepFilletAPI_MakeChamfer foot_chamfer(body);
  foot_chamfer.Add(2.6, 2.0, foot.front(), side);
  foot_chamfer.Build();
  CHECK(foot_chamfer.IsDone());
  if (!foot_chamfer.IsDone()) {
    return;
  }
  body = foot_chamfer.Shape();
  const std::vector<TopoDS_Edge> top = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.X() - 0.5) < 1e-6 && p.Z() > 2.65;
  });
  std::vector<TopoDS_Edge> tip_edge;
  for (const TopoDS_Edge& edge : top) {
    if (BRepAdaptor_Curve(edge).GetType() != GeomAbs_Line) {
      tip_edge.push_back(edge);
    }
  }
  CHECK(tip_edge.size() == 1u);
  if (tip_edge.size() != 1u) {
    return;
  }
  TopoDS_Shape result;
  if (by_angle) {
    TopoDS_Face wall;
    for (TopExp_Explorer ex(body, TopAbs_FACE); ex.More(); ex.Next()) {
      BRepAdaptor_Surface surface(TopoDS::Face(ex.Current()));
      if (surface.GetType() == GeomAbs_Plane &&
          std::abs(std::abs(surface.Plane().Axis().Direction().X()) - 1) < 1e-9 &&
          std::abs(surface.Plane().Location().X() - 0.5) < 1e-9) {
        wall = TopoDS::Face(ex.Current());
      }
    }
    BRepFilletAPI_MakeChamfer maker(body);
    maker.AddDA(0.6, kPi / 4.0, tip_edge.front(), wall);
    try {
      OCC_CATCH_SIGNALS
      maker.Build();
    } catch (const Standard_Failure& failure) {
      std::fprintf(stderr, "chamfer overflow test: %s\n", failure.what());
    }
    if (maker.IsDone()) {
      result = maker.Shape();
    }
  } else {
    result = chamfered(body, tip_edge, 0.6);
  }
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(pcurves_fit(result));
  // The sections square to the edge E(u) = (0.5, tip(u)): the triangle of
  // legs 0.6 along -x (on the tip) and m (in the side, square to the edge,
  // downward), less its part inside the foot's chamfer extended.
  const auto removed = [](const gp_Pnt& E, const gp_Vec& m, double& cu, double& cw) {
    const auto f = [&](double u, double w) {
      const gp_Pnt P = E.Translated(gp_Vec(-u, 0, 0) + m * w);
      return P.Z() - (2.6 - 1.3 * (P.X() - 0.5));
    };
    const std::pair<double, double> corners[3] = {{0, 0}, {0.6, 0}, {0, 0.6}};
    std::vector<std::pair<double, double>> kept;
    for (int i = 0; i < 3; ++i) {
      const auto a = corners[i], b = corners[(i + 1) % 3];
      const double fa = f(a.first, a.second), fb = f(b.first, b.second);
      if (fa >= 0) {
        kept.push_back(a);
      }
      if ((fa >= 0) != (fb >= 0)) {
        const double t = fa / (fa - fb);
        kept.push_back({a.first + t * (b.first - a.first), a.second + t * (b.second - a.second)});
      }
    }
    double area = 0.0, sx = 0.0, sy = 0.0;
    for (std::size_t i = 0; i < kept.size(); ++i) {
      const auto a = kept[i], b = kept[(i + 1) % kept.size()];
      const double cross = a.first * b.second - b.first * a.second;
      area += 0.5 * cross;
      sx += (a.first + b.first) * cross;
      sy += (a.second + b.second) * cross;
    }
    cu = area != 0.0 ? sx / (6.0 * area) : 0.0;
    cw = area != 0.0 ? sy / (6.0 * area) : 0.0;
    return std::abs(area);
  };
  const int n = 6000;
  double expected = 0.0;
  for (int i = 0; i <= n; ++i) {
    const double u = 3.0 * i / n;
    gp_Pnt E;
    gp_Vec d1, d2;
    tip->D2(u, E, d1, d2);
    E.SetX(0.5);
    const gp_Vec t = d1.Normalized();
    gp_Vec m = gp_Vec(1, 0, 0).Crossed(t);
    if (m.Z() > 0) {
      m.Reverse();
    }
    double cu = 0.0, cw = 0.0;
    const double area = removed(E, m, cu, cw);
    const gp_Vec curvature = (d2 - t * d2.Dot(t)) / d1.SquareMagnitude();
    const double weight = (i == 0 || i == n) ? 1.0 : (i % 2 == 1 ? 4.0 : 2.0);
    expected += weight / 3.0 * area * (1.0 - curvature.Dot(gp_Vec(-cu, 0, 0) + m * cw)) * d1.Magnitude() * 3.0 / n;
  }
  // (The chamfer's surface is walked: within its approximation.)
  CHECK_NEAR_TOL(exact_volume_of(body) - exact_volume_of(result), expected, 1e-4);
}

// The outline of a ring of 12 teeth on z = 0 (patch 0650): between the
// teeth arcs of the root circle (radius 11, each 6 degrees wide, centred at
// 0, 30, 60 ... degrees), straight flanks out to the tip circle (radius
// 12.5, arcs 10 degrees wide), the flanks' corners rounded, 0.5 at the root
// and 0.4 at the tip: round each tooth seven faces.
TopoDS_Face teeth_outline() {
  const int n = 12;
  const double root = 11.0, tip = 12.5;
  const double pitch = 2.0 * kPi / n, half_root = 3.0 * kPi / 180.0, half_tip = 5.0 * kPi / 180.0;
  const auto at = [](double r, double a) { return gp_Pnt(r * std::cos(a), r * std::sin(a), 0.0); };
  const auto arc = [](double r, double a0, double a1) {
    const gp_Circ circle(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), r);
    return BRepBuilderAPI_MakeEdge(circle, a0, a1).Edge();
  };
  BRepBuilderAPI_MakeWire wire;
  for (int k = 0; k < n; ++k) {
    const double c = k * pitch;
    const double tooth = c + 0.5 * pitch;
    wire.Add(arc(root, c - half_root, c + half_root));
    wire.Add(BRepBuilderAPI_MakeEdge(at(root, c + half_root), at(tip, tooth - half_tip)).Edge());
    wire.Add(arc(tip, tooth - half_tip, tooth + half_tip));
    wire.Add(BRepBuilderAPI_MakeEdge(at(tip, tooth + half_tip), at(root, c + pitch - half_root)).Edge());
  }
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  BRepFilletAPI_MakeFillet2d rounded(face);
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> vertices;
  TopExp::MapShapes(face, TopAbs_VERTEX, vertices);
  for (int i = 1; i <= vertices.Extent(); ++i) {
    const TopoDS_Vertex& vertex = TopoDS::Vertex(vertices(i));
    const gp_Pnt p = BRep_Tool::Pnt(vertex);
    const double r = std::hypot(p.X(), p.Y());
    rounded.AddFillet(vertex, r < 0.5 * (root + tip) ? 0.5 : 0.4);
  }
  rounded.Build();
  return TopoDS::Face(rounded.Shape());
}

// A flange of radius 14.5 from z = -6 to 0 with the 12 teeth standing on it
// up to z = 8 (`split`: the flange's top circle in two edges, its vertices
// at 0 and 180 degrees, as a body built of two halves keeps it).
TopoDS_Shape flange_under_teeth(bool split) {
  const TopoDS_Shape teeth = BRepPrimAPI_MakePrism(teeth_outline(), gp_Vec(0, 0, 8)).Shape();
  const gp_Ax2 axis(gp_Pnt(0, 0, -6), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0));
  NCollection_List<TopoDS_Shape> arguments, tools;
  if (split) {
    arguments.Append(BRepPrimAPI_MakeCylinder(axis, 14.5, 6.0, kPi).Shape());
    tools.Append(
        BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -6), gp_Dir(0, 0, 1), gp_Dir(-1, 0, 0)), 14.5, 6.0, kPi).Shape());
  } else {
    arguments.Append(BRepPrimAPI_MakeCylinder(axis, 14.5, 6.0).Shape());
  }
  tools.Append(teeth);
  BRepAlgoAPI_Fuse op;
  op.SetArguments(arguments);
  op.SetTools(tools);
  op.Build();
  // (Faces merged, edges kept: the split circle stays in two.)
  ShapeUpgrade_UnifySameDomain unify(op.Shape(), false, true, false);
  unify.Build();
  return unify.Shape();
}

// The flange's top circle chamfered past the strip of its top between the
// circle and the teeth (patch 0650), as a pulley's or a pinion's flange is:
// on the top as far as radius `reach`, the second leg 0.6 down the
// flange's side (by distance and angle, or by two distances). The strip is
// narrower than that between the teeth only where the root circle is (and
// at `reach` 11 the chamfer ends exactly on the root arcs); under each
// tooth the contact runs past the strip into the tooth's seven faces, which
// grow down to the chamfer: the chain of faces cutting the blend over each
// gap is seven long, its cut on the root rounding is the arc of the
// rounding's cylinder nearest the blend's piece (the intersection's other
// arc runs round the cylinder's far side), the gaps end at the root arcs'
// vertices where `reach` is 11, and a root arc holds the closed spine's
// start (or, `split`, its two edges' vertices). The material taken away is
// the revolved triangle of the chamfer's section less the teeth extended
// down. Before, the chain of faces was not found (four at most), and the
// stripe was left to a walking that chamfers do not have.
void test_chamfer_of_a_flange_under_teeth(double reach, bool by_angle, bool split) {
  const TopoDS_Shape body = flange_under_teeth(split);
  TopoDS_Face top;
  for (TopExp_Explorer ex(body, TopAbs_FACE); ex.More(); ex.Next()) {
    BRepAdaptor_Surface surface(TopoDS::Face(ex.Current()));
    if (surface.GetType() == GeomAbs_Plane && std::abs(surface.Plane().Location().Z()) < 1e-9 &&
        std::abs(surface.Plane().Axis().Direction().Z()) > 0.5) {
      top = TopoDS::Face(ex.Current());
    }
  }
  const std::vector<TopoDS_Edge> rim = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z()) < 1e-6 && std::abs(std::hypot(p.X(), p.Y()) - 14.5) < 1e-6;
  });
  CHECK(!top.IsNull());
  CHECK(rim.size() == (split ? 2u : 1u));
  if (top.IsNull() || rim.empty()) {
    return;
  }
  const double leg = 14.5 - reach, down = 0.6;
  BRepFilletAPI_MakeChamfer maker(body);
  if (by_angle) {
    maker.AddDA(leg, std::atan2(down, leg), rim.front(), top);
  } else {
    maker.Add(leg, down, rim.front(), top);
  }
  for (std::size_t i = 1; i < rim.size(); ++i) {
    if (maker.Contour(rim[i]) == 0) {
      maker.Add(leg, down, rim[i], top);
    }
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "chamfer overflow test: %s\n", failure.what());
  }
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    return;
  }
  const TopoDS_Shape result = maker.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(pcurves_fit(result));
  // The section's triangle revolved, less the teeth extended down.
  BRepBuilderAPI_MakePolygon section(gp_Pnt(reach, 0, 0), gp_Pnt(14.5 + 1.0, 0, 0),
                                     gp_Pnt(14.5 + 1.0, 0, -down * (14.5 + 1.0 - reach) / leg), true);
  const TopoDS_Shape wedge = BRepPrimAPI_MakeRevol(BRepBuilderAPI_MakeFace(section.Wire(), true).Face(),
                                                   gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)))
                                 .Shape();
  TopoDS_Face low = teeth_outline();
  gp_Trsf move;
  move.SetTranslation(gp_Vec(0, 0, -1.0));
  low = TopoDS::Face(BRepBuilderAPI_Transform(low, move, true).Shape());
  const TopoDS_Shape teeth = BRepPrimAPI_MakePrism(low, gp_Vec(0, 0, 2.0)).Shape();
  const TopoDS_Shape expected = cut(body, cut(wedge, teeth));
  CHECK_NEAR_TOL(exact_volume_of(result), exact_volume_of(expected), 1e-7);
}

// A V groove (mouth 2 wide on the top z = 0, flanks down to a tip rounded
// with 0.3, 1.2 deep) whose profile lies in the plane x = 0 centred at y =
// yc, swept from there to x = x1 along (1, -0.1, 0) (a thread's turn
// drifting along the axis).
TopoDS_Shape slanted_groove(double yc, double x1) {
  const double r = 0.3, cz = -0.9, half = 1.0, slope = -0.1;
  const double a = std::atan2(-cz, half) - std::acos(r / std::hypot(half, cz));
  const gp_Pnt right(0, yc + r * std::cos(a), cz + r * std::sin(a)), left(0, yc - r * std::cos(a), cz + r * std::sin(a));
  BRepBuilderAPI_MakeWire profile;
  profile.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(0, yc - half, 0), gp_Pnt(0, yc + half, 0)).Edge());
  profile.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(0, yc + half, 0), right).Edge());
  profile.Add(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(0, yc, cz), gp_Dir(1, 0, 0)), r), left, right).Edge());
  profile.Add(BRepBuilderAPI_MakeEdge(left, gp_Pnt(0, yc - half, 0)).Edge());
  return BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(profile.Wire(), true).Face(), gp_Vec(x1, slope * x1, 0)).Shape();
}

void test_chamfer_of_a_groove_end_wall(double along, double down) {
  // A block with two such grooves side by side, their mouths meeting at y =
  // 1: the first ends at x = 0 in a flat wall, the second runs on. The edge
  // where the wall meets the top is chamfered, `along` on the top, `down`
  // the wall, below the flanks' ends: both contacts leave their faces
  // before the edge's ends (port patch 0671). The flanks, grown on past
  // the wall, cut the chamfer across its whole width: at y < 0 the first
  // groove's flank meets the contact on the top on its mouth edge carried
  // on past the wall; at y > 0 where the second groove's mouth leaves the
  // top. Before, the corners at both ends failed.
  BRepAlgoAPI_Cut first(BRepPrimAPI_MakeBox(gp_Pnt(-5, -3, -3), gp_Pnt(5, 5, 0)).Shape(), slanted_groove(0.0, -6.0));
  first.SimplifyResult();
  // (The second from x = -6 to 6, centred at y = 2 at x = 0.)
  gp_Trsf back;
  back.SetTranslation(gp_Vec(-6.0, 0.6, 0.0));
  const TopoDS_Shape second = BRepBuilderAPI_Transform(slanted_groove(2.0, 12.0), back, true).Shape();
  BRepAlgoAPI_Cut grooves(first.Shape(), second);
  grooves.SimplifyResult();
  const TopoDS_Shape body = grooves.Shape();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const std::vector<TopoDS_Edge> foot = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.X()) < 1e-6 && std::abs(p.Z()) < 1e-6 && std::abs(p.Y()) < 1.0 + 1e-6;
  });
  CHECK(foot.size() == 1u);
  if (foot.size() != 1u) {
    return;
  }
  // The top beside the edge, and the wall.
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndUniqueAncestors(body, TopAbs_EDGE, TopAbs_FACE, faces_of);
  TopoDS_Face top, wall;
  for (const TopoDS_Shape& f : faces_of.FindFromKey(foot.front())) {
    BRepAdaptor_Surface surface(TopoDS::Face(f));
    (std::abs(surface.Plane().Axis().Direction().Z()) > 0.5 ? top : wall) = TopoDS::Face(f);
  }
  CHECK(!top.IsNull() && !wall.IsNull());
  if (top.IsNull() || wall.IsNull()) {
    return;
  }
  BRepFilletAPI_MakeChamfer maker(body);
  maker.Add(along, down, foot.front(), top);
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "chamfer overflow test: %s\n", failure.what());
  }
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    return;
  }
  const TopoDS_Shape result = maker.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(pcurves_fit(result));
  // The chamfer's wedge within the first groove carried on past the wall.
  BRepBuilderAPI_MakePolygon section(gp_Pnt(0, -4, 0), gp_Pnt(along, -4, 0), gp_Pnt(0, -4, -down), true);
  const TopoDS_Shape wedge =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(section.Wire(), true).Face(), gp_Vec(0, 8, 0)).Shape();
  const TopoDS_Shape removed = BRepAlgoAPI_Common(slanted_groove(0.0, 6.0), wedge).Shape();
  CHECK_NEAR_TOL(exact_volume_of(body) - exact_volume_of(result), exact_volume_of(removed), 1e-7);
  // Mitcad's chamfer: the pieces' contacts on the top and on the flanks
  // meet at vertices of their own, which it joins (one edge between the
  // chamfer's face and the top, as in the files).
  std::vector<mitcad::geometry::Shape::NamedFace> named;
  std::string top_name, wall_name;
  int k = 0;
  for (TopExp_Explorer ex(body, TopAbs_FACE); ex.More(); ex.Next()) {
    const std::string name = "F1:import(" + std::to_string(++k) + ")";
    named.push_back({ex.Current(), {name}});
    if (ex.Current().IsSame(top)) {
      top_name = name;
    } else if (ex.Current().IsSame(wall)) {
      wall_name = name;
    }
  }
  const mitcad::geometry::Shape shape(body, named);
  mitcad::geometry::ChamferSpec spec;
  spec.type = mitcad::geometry::ChamferType::TwoDistances;
  spec.distance = along;
  spec.distance2 = down;
  spec.reference_face = top_name;
  const mitcad::geometry::ShapePtr bevelled = mitcad::geometry::chamfer("F2", shape, {edge(top_name, wall_name)}, spec);
  CHECK(BRepCheck_Analyzer(bevelled->occt()).IsValid());
  const std::string bevel_face = "F2:chamfer(" + edge(top_name, wall_name) + ")";
  int with_top = 0;
  for (int e = 0; e < bevelled->edge_count(); ++e) {
    const std::array<int, 2> faces = bevelled->edge_faces(e);
    if (faces[0] < 0 || faces[1] < 0) {
      continue;
    }
    bool on_bevel = false, on_top = false;
    for (const int f : faces) {
      for (const std::string& name : bevelled->face_names(f)) {
        on_bevel = on_bevel || name == bevel_face;
        on_top = on_top || name == top_name;
      }
    }
    with_top += on_bevel && on_top ? 1 : 0;
  }
  CHECK(with_top == 1);
}

// A half-round boss of radius 5 (x <= 0) on a plate, its top a plane
// sloping down from z = 9.5 at x = 0 by `slope` per unit of -x; its foot
// chamfered `foot` up its round side and 1 on the plate, its top's arc
// chamfered by `distance`. The top's chamfer is walked, and patch 0531
// computes it again over the round side unbounded. With the body moved by a
// location (as an assembly's component is), the side's surface is a
// cylinder held with that location: the unbounded copy took a copy of the
// cylinder moved by it, on which the spine's edge has no p-curve, and the
// try evaluated a null p-curve (a signal, mitcad#1986). The chamfer of the
// moved body is that of the body as made: valid, the same volume taken
// away.
void test_chamfer_of_a_sloped_boss_top_moved(double foot, double distance, double slope) {
  const double radius = 5.0, top = 9.5;
  TopoDS_Shape boss =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1), gp_Dir(0, 1, 0)), radius, top + 5.0, kPi)
          .Shape();
  const TopoDS_Face slope_face =
      BRepBuilderAPI_MakeFace(gp_Pln(gp_Pnt(0, 0, top), gp_Dir(-slope, 0, 1))).Face();
  boss = cut(boss, BRepPrimAPI_MakeHalfSpace(slope_face, gp_Pnt(0, 0, 100)).Solid());
  TopoDS_Shape body = fuse(BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, -5), gp_Pnt(10, 10, 0)).Shape(), boss);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const std::vector<TopoDS_Edge> foot_arc = edges_where(body, [&](const gp_Pnt& p) {
    return std::abs(p.Z()) < 1e-6 && std::abs(std::hypot(p.X(), p.Y()) - radius) < 1e-6;
  });
  TopoDS_Face side;
  for (TopExp_Explorer ex(body, TopAbs_FACE); ex.More(); ex.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(ex.Current())).GetType() == GeomAbs_Cylinder) {
      side = TopoDS::Face(ex.Current());
    }
  }
  CHECK(foot_arc.size() == 1u);
  CHECK(!side.IsNull());
  if (foot_arc.size() != 1u || side.IsNull()) {
    return;
  }
  BRepFilletAPI_MakeChamfer foot_chamfer(body);
  foot_chamfer.Add(foot, 1.0, foot_arc.front(), side);
  foot_chamfer.Build();
  CHECK(foot_chamfer.IsDone());
  if (!foot_chamfer.IsDone()) {
    return;
  }
  body = foot_chamfer.Shape();
  gp_Trsf shift, turn;
  shift.SetTranslation(gp_Vec(3, -2, 1));
  turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(1, 1, 1)), 0.3);
  const gp_Trsf move = turn * shift;
  double removed[2] = {0.0, 0.0};
  for (const bool moved : {false, true}) {
    const TopoDS_Shape shape = moved ? body.Moved(TopLoc_Location(move)) : body;
    const gp_Trsf back = moved ? move.Inverted() : gp_Trsf();
    const std::vector<TopoDS_Edge> arc = edges_where(shape, [&](const gp_Pnt& q) {
      const gp_Pnt p = q.Transformed(back);
      return p.Z() > top - slope * radius - 1e-3 && std::abs(std::hypot(p.X(), p.Y()) - radius) < 1e-6;
    });
    CHECK(arc.size() == 1u);
    if (arc.size() != 1u) {
      return;
    }
    const TopoDS_Shape result = chamfered(shape, arc, distance);
    CHECK(!result.IsNull());
    if (result.IsNull()) {
      return;
    }
    CHECK(BRepCheck_Analyzer(result).IsValid());
    removed[moved ? 1 : 0] = volume_of(shape) - volume_of(result);
  }
  CHECK(removed[0] > 0.0);
  CHECK_NEAR(removed[1], removed[0]);
}

void chamfer_overflow_tests() {
  guarded("test_chamfer_as_wide_as_a_step_ring", test_chamfer_as_wide_as_a_step_ring);
  guarded("test_step_ring_rim_blends_past_the_seam", test_step_ring_rim_blends_past_the_seam);
  guarded("test_chamfer_as_wide_as_a_lug_in_a_bore", test_chamfer_as_wide_as_a_lug_in_a_bore);
  // (The lug's ends away from the bore's seam, its end and its start on it.)
  for (const bool fillet : {false, true}) {
    for (const auto& [start, seam] : {std::pair<double, double>{112.5, 0.0}, {225.0, 0.0}, {0.0, 0.0}}) {
      guarded("test_lug_in_a_bore_blends_on_its_seam", [&, start = start, seam = seam] {
        test_lug_in_a_bore_blends_on_its_seam(start, seam, fillet);
      });
    }
  }
  for (const bool bottom_first : {false, true}) {
    guarded("test_chamfer_of_a_rib_cut_by_its_sides", [&] { test_chamfer_of_a_rib_cut_by_its_sides(bottom_first); });
  }
  // (Not the longer boss with its seam in the gap: patch 0145.)
  for (const auto& [exact, away] : {std::pair<bool, bool>{true, true}, {false, true}, {true, false}}) {
    guarded("test_chamfer_of_a_boss_beyond_a_plate_corner",
            [&, exact = exact, away = away] { test_chamfer_of_a_boss_beyond_a_plate_corner(exact, away); });
  }
  for (const bool exact : {true, false}) {
    for (const bool away : {true, false}) {
      guarded("test_boss_foot_chamfer_across_its_seam", [&] { test_boss_foot_chamfer_across_its_seam(exact, away); });
    }
  }
  for (const bool seam_at_corner : {false, true}) {
    guarded("test_chamfers_crossing_beyond_their_common_face",
            [&] { test_chamfers_crossing_beyond_their_common_face(seam_at_corner); });
  }
  for (const double ledge_height : {0.5, 1.0}) {
    guarded("test_chamfer_as_wide_as_a_ledge_along_a_line_and_an_arc",
            [&] { test_chamfer_as_wide_as_a_ledge_along_a_line_and_an_arc(ledge_height); });
  }
  for (const double seam : {180.0, 90.0}) {
    for (const bool by_angle : {false, true}) {
      guarded("test_chamfer_as_long_as_a_boss", [&] { test_chamfer_as_long_as_a_boss(seam, by_angle); });
    }
  }
  // (wall, ledge, lip, rounded)
  for (const auto& [wall, ledge, lip, fillet] : {std::tuple<double, double, bool, bool>{1.5, 1.5, false, false},
                                                 {3.0, 1.5, false, false},
                                                 {1.5, 3.0, false, false},
                                                 {1.5, 1.5, true, false},
                                                 {3.0, 1.5, true, false},
                                                 {1.5, 3.0, true, false},
                                                 {1.5, 1.5, false, true},
                                                 {1.5, 1.5, true, true}}) {
    guarded("test_wall_foot_round_a_stepped_pocket", [&, wall = wall, ledge = ledge, lip = lip, fillet = fillet] {
      test_wall_foot_round_a_stepped_pocket(wall, ledge, lip, fillet);
    });
  }
  for (const double distance : {1.0, 0.6}) {
    guarded("test_chamfer_of_a_rounded_crease", [&] { test_chamfer_of_a_rounded_crease(10.0, distance); });
  }
  for (const double rounding : {0.0, 0.5}) {
    guarded("test_chamfer_as_wide_as_a_flange_with_a_rounded_rim",
            [&] { test_chamfer_as_wide_as_a_flange_with_a_rounded_rim(rounding); });
  }
  // (split, split_side, rounded)
  for (const auto& [split, split_side, fillet] : {std::tuple<bool, bool, bool>{false, false, false},
                                                  {true, false, false},
                                                  {true, true, false},
                                                  {false, false, true},
                                                  {true, true, true}}) {
    guarded("test_chamfer_consuming_a_strip_along_a_chain_of_edges",
            [&, split = split, split_side = split_side, fillet = fillet] {
              test_chamfer_consuming_a_strip_along_a_chain_of_edges(split, split_side, fillet);
            });
  }
  // (seam in degrees, the plate's turn in radians)
  for (const auto& [seam, turn] : {std::pair<double, double>{0.0, 0.0},
                                   {10.0, 0.0},
                                   {90.0, 0.0},
                                   {45.0, 0.0},
                                   {0.0, 1e-8},
                                   {90.0, 1e-8}}) {
    guarded("test_chamfer_of_a_post_beyond_a_square_plate", [&, seam = seam, turn = turn] {
      test_chamfer_of_a_post_beyond_a_square_plate(seam, turn);
    });
  }
  // (the tip's dip, the chamfer by distance and angle)
  for (const double dip : {2.8, 2.6}) {
    for (const bool by_angle : {false, true}) {
      guarded("test_chamfer_wider_than_a_fin_side_strip",
              [&] { test_chamfer_wider_than_a_fin_side_strip(dip, by_angle); });
    }
  }
  // (the chamfer's reach on the flange's top, by distance and angle, the
  // top circle in two edges)
  for (const auto& [reach, by_angle, split] : {std::tuple<double, bool, bool>{11.05, true, false},
                                               {11.05, false, true},
                                               {11.0, false, false},
                                               {11.0, true, true}}) {
    guarded("test_chamfer_of_a_flange_under_teeth", [&, reach = reach, by_angle = by_angle, split = split] {
      test_chamfer_of_a_flange_under_teeth(reach, by_angle, split);
    });
  }
  // (the chamfer's reach on the top and down the wall)
  for (const auto& [along, down] : {std::pair<double, double>{1.5, 1.15}, {2.0, 1.18}, {1.0, 1.12}}) {
    guarded("test_chamfer_of_a_groove_end_wall",
            [&, along = along, down = down] { test_chamfer_of_a_groove_end_wall(along, down); });
  }
  // (the boss's seam in degrees: before the gap, failing without 0941, or
  // away from it; the boss's length: as the chamfer, or longer)
  for (const double seam : {180.0, 150.0, 120.0, 300.0}) {
    for (const double length : {13.0, 15.0}) {
      guarded("test_chamfer_of_a_boss_beyond_a_slanted_plate_corner",
              [&] { test_chamfer_of_a_boss_beyond_a_slanted_plate_corner(seam, length); });
    }
  }
  // (the foot's height up the boss's side, the top's chamfer, the top's slope)
  for (const auto& [foot, distance, slope] :
       {std::tuple<double, double, double>{6.0, 1.5, 0.3}, {3.0, 0.8, 0.1}}) {
    guarded("test_chamfer_of_a_sloped_boss_top_moved", [&, foot = foot, distance = distance, slope = slope] {
      test_chamfer_of_a_sloped_boss_top_moved(foot, distance, slope);
    });
  }
}

} // namespace test
