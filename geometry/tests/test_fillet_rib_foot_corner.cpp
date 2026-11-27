// SPDX-License-Identifier: MIT
// The corner where a rib's rounding ends at its foot beside a channel's rim,
// with OCCT's fillet alone (mitcad#2038, the port's patch 1805).
//
// A block with a round channel (radius 2.2, its axis along y at z = 0) cut
// open at the top by the flat z = 1.1, and a rib bridging the channel above
// the flat from y = 0 to y = 2: the rib's underside is the channel's
// cylinder above the flat, a face of its own beside the channel's wall below
// (their common edge along the rim is smooth). The edge where the rib's end
// face (y = 2) meets its underside is rounded with 0.5 on that side. It ends at its foot on the rim, a vertex of four
// edges: the rounded one, the rib's end on the flat (a concave edge), the
// flat's rim past the rib (sharp) and the smooth edge under the rib.
//
// Before patch 1805, OCCT's corner there set back 0.5 along every sharp
// edge, the concave one too, and its plate turned the concave corner
// between the rib's end and the flat: it reached 0.02-0.03 mm out of the
// body above the flat past the rib (the bent guide of GitHub #456, where
// the ribs' feet are such corners). Now the corner is set back along the
// concave edge only and its plate closes against the faces the rounding
// takes away.
//
// Checks: each rounding builds a valid result that takes material away;
// every face it made lies inside the body, no point of it farther out than
// the face's own tolerance (and less than 0.005); the flat past the rib
// keeps its extent.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>

#include "bundled_test.hpp"

namespace {

int failures = 0;

void check(bool ok, const char* what) {
  if (!ok) {
    std::fprintf(stderr, "check failed: %s\n", what);
    ++failures;
  }
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// The block with the open channel and the rib bridging it. The rib's
// underside is a cylinder of radius 2.6 tangent to the channel's along its
// rim on the side x > 0, so that the two are faces of their own with a
// smooth edge between them there.
TopoDS_Shape guide() {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-3.7, -4.0, -3.7), gp_Pnt(3.7, 6.0, 1.1)).Shape();
  const TopoDS_Shape rib = BRepPrimAPI_MakeBox(gp_Pnt(-3.7, 0.0, 1.1), gp_Pnt(3.7, 2.0, 2.2)).Shape();
  const gp_Dir along(0.0, 1.0, 0.0);
  const TopoDS_Shape channel = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0.0, -5.0, 0.0), along), 2.2, 12.0).Shape();
  // The rim on the side x > 0: (sqrt(2.2^2 - 1.1^2), 1.1); the arch's axis
  // 0.4 back from the channel's along the radius through it.
  const double rim = std::sqrt(2.2 * 2.2 - 1.1 * 1.1);
  const gp_Pnt arch_axis(-0.4 * rim / 2.2, -5.0, -0.4 * 1.1 / 2.2);
  const TopoDS_Shape arch = BRepPrimAPI_MakeCylinder(gp_Ax2(arch_axis, along), 2.6, 12.0).Shape();
  const TopoDS_Shape walls = BRepAlgoAPI_Cut(block, channel).Shape();
  const TopoDS_Shape bridge = BRepAlgoAPI_Cut(rib, arch).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(walls, bridge).Shape(), true, true, false);
  unify.Build();
  return unify.Shape();
}

// The edge between the rib's end face (y = 2) and its underside on the side
// x of sign `side`: a curved edge in the plane y = 2 from the rim
// (|x| = 1.905, z = 1.1) up to z = 2.2.
TopoDS_Edge foot_edge(const TopoDS_Shape& body, double side) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& e = TopoDS::Edge(edges(i));
    if (BRepAdaptor_Curve(e).GetType() != GeomAbs_Circle) {
      continue;
    }
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(e));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(e));
    const gp_Pnt& low = a.Z() < b.Z() ? a : b;
    const gp_Pnt& high = a.Z() < b.Z() ? b : a;
    if (std::abs(a.Y() - 2.0) < 1e-6 && std::abs(b.Y() - 2.0) < 1e-6 && std::abs(low.Z() - 1.1) < 1e-6 &&
        std::abs(high.Z() - 2.2) < 1e-6 && low.X() * side > 1.0) {
      return e;
    }
  }
  return TopoDS_Edge();
}

// How far the faces of `result` that are not faces of `body` reach out of
// `body` beyond their own tolerances (a grid of points over each face).
double reach_beyond_tolerance(const TopoDS_Shape& body, const TopoDS_Shape& result, double& worst) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> kept;
  TopExp::MapShapes(body, TopAbs_FACE, kept);
  BRepClass3d_SolidClassifier classifier(body);
  double beyond = 0.0;
  worst = 0.0;
  for (TopExp_Explorer ex(result, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& face = TopoDS::Face(ex.Current());
    if (kept.Contains(face)) {
      continue;
    }
    TopLoc_Location location;
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
    bool old = false;
    for (int k = 1; k <= kept.Extent() && !old; ++k) {
      TopLoc_Location other;
      old = BRep_Tool::Surface(TopoDS::Face(kept(k)), other) == surface;
    }
    if (old) {
      continue;
    }
    const BRepAdaptor_Surface s(face);
    BRepTopAdaptor_FClass2d inside(face, 1e-9);
    const double tolerance = BRep_Tool::Tolerance(face);
    const int n = 30;
    for (int i = 0; i <= n; ++i) {
      for (int j = 0; j <= n; ++j) {
        const double u = s.FirstUParameter() + (s.LastUParameter() - s.FirstUParameter()) * i / n;
        const double v = s.FirstVParameter() + (s.LastVParameter() - s.FirstVParameter()) * j / n;
        if (inside.Perform(gp_Pnt2d(u, v)) != TopAbs_IN) {
          continue;
        }
        const gp_Pnt p = s.Value(u, v);
        classifier.Perform(p, 1e-7);
        if (classifier.State() != TopAbs_OUT) {
          continue;
        }
        BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), body);
        if (!distance.IsDone()) {
          continue;
        }
        worst = std::max(worst, distance.Value());
        beyond = std::max(beyond, distance.Value() - tolerance);
      }
    }
  }
  return beyond;
}

// The area of the flat faces at z = 1.1.
double flat_area(const TopoDS_Shape& shape) {
  double area = 0.0;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& face = TopoDS::Face(ex.Current());
    const BRepAdaptor_Surface s(face);
    if (s.GetType() != GeomAbs_Plane || std::abs(std::abs(s.Plane().Axis().Direction().Z()) - 1.0) > 1e-9 ||
        std::abs(s.Plane().Location().Z() - 1.1) > 1e-9) {
      continue;
    }
    GProp_GProps props;
    BRepGProp::SurfaceProperties(face, props);
    area += props.Mass();
  }
  return area;
}

void test_rounding(const TopoDS_Shape& body, const std::vector<double>& sides, const char* what) {
  BRepFilletAPI_MakeFillet maker(body);
  std::vector<TopoDS_Edge> edges;
  for (const double side : sides) {
    edges.push_back(foot_edge(body, side));
    check(!edges.back().IsNull(), "the rib's edge at its foot is found");
    if (edges.back().IsNull()) {
      return;
    }
  }
  TopoDS_Shape result;
  try {
    OCC_CATCH_SIGNALS
    for (const TopoDS_Edge& edge : edges) {
      maker.Add(0.5, edge);
    }
    maker.Build();
    if (maker.IsDone()) {
      result = maker.Shape();
    }
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "%s: fillet: %s\n", what, failure.what());
  }
  check(!result.IsNull(), "the rounding builds");
  if (result.IsNull()) {
    return;
  }
  check(BRepCheck_Analyzer(result).IsValid(), "the result is valid");
  const double taken = volume_of(body) - volume_of(result);
  double worst = 0.0;
  const double beyond = reach_beyond_tolerance(body, result, worst);
  std::printf("%s: takes %.6f mm3, its faces reach %.6f mm out of the body (%.6f beyond their tolerances)\n",
              what, taken, worst, beyond);
  check(taken > 0.0, "the rounding takes material away");
  check(beyond <= 1e-6 && worst < 0.005, "the faces it made stay inside the body");
  // The flat loses no more than the patch under the rounding's end: before,
  // the plate replaced a piece of it past the rib's end, 0.5 along the rim.
  check(flat_area(result) >= flat_area(body) - 1e-6, "the flat keeps its extent past the rib");
}

}  // namespace

MITCAD_BUNDLED_TEST(fillet_rib_foot_corner) {
  const TopoDS_Shape body = guide();
  check(BRepCheck_Analyzer(body).IsValid(), "the guide is valid");
  test_rounding(body, {1.0}, "one foot");
  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("ok\n");
  return 0;
}
