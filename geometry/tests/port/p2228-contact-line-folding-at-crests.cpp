// SPDX-License-Identifier: MIT
// Port patch 2228: a rounding whose contact line on a face folds over
// itself, where the edge bends towards that face more tightly than the
// ball touches it from the edge, is walked across the cusps where its
// contact turns back, and as several pieces, the twisted one across each
// fold between untwisted ones, which OCCT fills between them
// (PerformSetOfKGen's reprocessing of twisted pieces).
//
// The body: the band between y = 12 + 8 sin(2 pi (x + 30) / 30) and the
// same curve 24 lower, x from -30 to 30 (two waves), 10 high (z from 0 to
// 10), made with OCCT alone (the waves interpolated through 121 points).
// Its walls bend at the crests with a radius of 2.85 mm (8 (2 pi / 30)^2
// = 1 / 2.85). Its top edge along the upper wave (y = 12 at both ends) is
// rounded with r: the ball touches the top face r from the edge, and at
// each of the edge's two crests (x = -22.5 and 7.5, where the material
// lies inside the bend) the contact line on the top face folds over
// itself above r = 2.85. Before the patch the walk stopped at the first
// cusp, its steps across it refused as steps back: 3, 5 and 8.5 failed
// (the walk between the two faces, one piece computed). At 2.5 it built
// before and does now. The volumes are OCCT's Gauss-Kronrod integration's:
// its default rule with an accuracy gave the band 15382.3, not 14400.
//
// Checked at 2.5, 3, 5 and 8.5 (the largest radius another program builds
// on the reported model of this shape, below the 10 mm wall): OCCT's
// fillet is done, one valid solid, the volume less than the band's, by
// less than the corner's square r^2 along the edge and by more than half
// the corner's section (1 - pi / 4) r^2 along it, falling as r grows; at
// 2.5 the section swept along the edge exactly (within 0.2 % of what it
// takes: the edge turns as much one way as the other, so the volume of a
// section moved along it is its area times the edge's length).

#include <cmath>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_HArray1.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

constexpr double kBand = 14400.0; // 24 x 60 x 10

// The volume by OCCT's adaptive Gauss-Kronrod integration span by span
// (as geometry/tests/test_accurate_volumes.cpp takes its reference): the
// default rule with an accuracy (port::volume_of) gave the band 15382.3
// instead of 14400, 7 % off on these B-spline walls.
double volume_gk(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumePropertiesGK(shape, props, 1.0e-9, false, true);
  return props.Mass();
}

// The wave y = offset + 8 sin(2 pi (x + 30) / 30) at z = 0 from x = -30
// to 30, or from 30 to -30 where `reversed`.
TopoDS_Edge wave(double offset, bool reversed) {
  constexpr int kPoints = 121;
  occ::handle<NCollection_HArray1<gp_Pnt>> points = new NCollection_HArray1<gp_Pnt>(1, kPoints);
  for (int i = 0; i < kPoints; ++i) {
    const double t = static_cast<double>(i) / (kPoints - 1);
    const double x = reversed ? 30.0 - 60.0 * t : -30.0 + 60.0 * t;
    points->SetValue(i + 1, gp_Pnt(x, offset + 8.0 * std::sin(2.0 * test::kPi * (x + 30.0) / 30.0), 0.0));
  }
  GeomAPI_Interpolate interpolate(points, false, 1e-9);
  interpolate.Perform();
  return BRepBuilderAPI_MakeEdge(interpolate.Curve()).Edge();
}

TopoDS_Shape band() {
  BRepBuilderAPI_MakeWire wire;
  wire.Add(wave(12.0, false));
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(30, 12, 0), gp_Pnt(30, -12, 0)).Edge());
  wire.Add(wave(-12.0, true));
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-30, -12, 0), gp_Pnt(-30, 12, 0)).Edge());
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, 10)).Shape();
}

// The top edge along the upper wave (both ends at y = 12, z = 10) and its
// length.
TopoDS_Edge top_wave_edge(const TopoDS_Shape& body, double& length) {
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge edge = TopoDS::Edge(ex.Current());
    const BRepAdaptor_Curve curve(edge);
    if (curve.GetType() == GeomAbs_Line) {
      continue;
    }
    const gp_Pnt a = curve.Value(curve.FirstParameter());
    const gp_Pnt b = curve.Value(curve.LastParameter());
    if (std::abs(a.Z() - 10.0) < 1e-6 && std::abs(b.Z() - 10.0) < 1e-6 && std::abs(a.Y() - 12.0) < 1e-6 &&
        std::abs(b.Y() - 12.0) < 1e-6) {
      length = GCPnts_AbscissaPoint::Length(curve);
      return edge;
    }
  }
  return TopoDS_Edge();
}

void test_wave_top_edge_rounded_past_its_crests() {
  const TopoDS_Shape body = band();
  CHECK(port::valid(body));
  const double before = volume_gk(body);
  std::printf("  the band: volume %.6f (OCCT's default rule with an accuracy: %.6f)\n", before,
              port::volume_of(body));
  CHECK_NEAR_TOL(before, kBand, 1e-6);
  double length = 0.0;
  const TopoDS_Edge edge = top_wave_edge(body, length);
  CHECK(!edge.IsNull());
  if (edge.IsNull()) {
    return;
  }
  const double corner = 1.0 - test::kPi / 4.0;
  double previous = before;
  for (const double r : {2.5, 3.0, 5.0, 8.5}) {
    BRepFilletAPI_MakeFillet maker(body);
    maker.Add(r, edge);
    try {
      OCC_CATCH_SIGNALS
      maker.Build();
    } catch (const Standard_Failure& failure) {
      std::printf("  r %g: raised '%s'\n", r, failure.what());
    }
    std::printf("  r %g: done %d, failure %d, %d computed surfaces\n", r, int(maker.IsDone()), int(maker.Failure()),
                maker.NbContours() > 0 ? maker.NbComputedSurfaces(1) : 0);
    CHECK(maker.IsDone());
    if (!maker.IsDone()) {
      continue;
    }
    const TopoDS_Shape rounded = maker.Shape();
    const double volume = volume_gk(rounded);
    const double removed = before - volume;
    std::printf("  r %g: volume %.6f, %d faces, valid %d\n", r, volume, port::count_of(rounded, TopAbs_FACE),
                int(port::valid(rounded)));
    CHECK(port::valid(rounded));
    CHECK(port::count_of(rounded, TopAbs_SOLID) == 1);
    CHECK(removed > 0.5 * corner * r * r * length);
    CHECK(removed < r * r * length);
    if (r == 2.5) {
      CHECK_NEAR_TOL(removed, corner * r * r * length, 2e-3);
    }
    CHECK(volume < previous);
    previous = volume;
  }
}

} // namespace

MITCAD_PORT_TEST("2228", test_wave_top_edge_rounded_past_its_crests);
