// SPDX-License-Identifier: MIT
// A rib's sloped edge rounded 1 degree from a bent guide's end, with OCCT's
// fillet alone (mitcad#2018, the port's patches 1801 and 1802).
//
// The case `fillet-corner-walk-over-vertex` of MITCAD_GEOMETRY_CASES:
// `bend-30.brep` and `bend-24.brep`, a reported design's body before its
// rounding with the bend radius set to 30 and to 24. The guide is a round
// tube (radius 2.2) with a flat top at z = 1.1, swept through a quarter
// turn about the z axis, a straight pad 2 long at each end; ribs stand on
// the flat, their sloped sides going on tangentially into the tube at the
// flat's rims. The rib nearest the end at y = 0 spans 1 to 5 degrees.
//
// bend-30: the rib's inner sloped edge rounded with 0.5. The corner where
// the rounding ends on the inner rim is set back 0.508 along the rim, which
// is 0.49 long from the rib to the bend's end: the set-back walks over the
// rim's end vertex onto the pad's rim (1801). Before, the plate's point lay
// past the rim's end and the result was invalid.
//
// bend-24: the rib's outer sloped edge rounded with 0.5. The walk back from
// the rounding's first section starts within the guide's tolerance of the
// rib side's edge with the tube; it ends on that edge (1802). Before, it
// ended at its first point and the fillet failed as a dragging walk.
//
// Checks: each fillet builds a valid result within the body's box (the
// corner reaches no further than the body does); it takes material away,
// less than three times the rounding's own section along the edge (the
// corners' plates add little; a corner that went astray past the rim's end
// took or added far more). In the reported design with all its roundings,
// the body's volume falls by 26.22 mm3 for each millimetre of bend radius
// across the radii where the corners walk over the vertex and where they
// do not (cli.fcstd_bend_reported).

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>

namespace {

int failures = 0;

void check(bool ok, const char* what) {
  if (!ok) {
    std::fprintf(stderr, "check failed: %s\n", what);
    ++failures;
  }
}

std::string cases_dir() {
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

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// The straight edge from the flat (z = 1.1) up to z = 2.2 whose lower end
// lies at `foot`, or a null edge.
TopoDS_Edge sloped_edge(const TopoDS_Shape& body, const gp_Pnt& foot) {
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    const BRepAdaptor_Curve c(e);
    if (c.GetType() != GeomAbs_Line) {
      continue;
    }
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(e));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(e));
    const gp_Pnt& low = a.Z() < b.Z() ? a : b;
    const gp_Pnt& high = a.Z() < b.Z() ? b : a;
    if (low.Distance(foot) < 1e-3 && std::abs(high.Z() - 2.2) < 1e-6) {
      return e;
    }
  }
  return TopoDS_Edge();
}

// The edge rounded with 0.5 by OCCT's fillet; a null shape when it fails.
TopoDS_Shape rounded(const TopoDS_Shape& body, const TopoDS_Edge& edge) {
  BRepFilletAPI_MakeFillet maker(body);
  maker.Add(0.5, edge);
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet: %s\n", failure.what());
    return TopoDS_Shape();
  }
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// The edge at `foot` rounded.
void test_body(const char* file, const gp_Pnt& foot) {
  const std::string dir = cases_dir();
  if (dir.empty()) {
    std::printf("%s: skipped (MITCAD_GEOMETRY_CASES is not set)\n", file);
    return;
  }
  TopoDS_Shape body;
  BRep_Builder builder;
  const std::string path = dir + "/fillet-corner-walk-over-vertex/" + file;
  if (!BRepTools::Read(body, path.c_str(), builder) || body.IsNull()) {
    std::printf("%s: skipped (no case fillet-corner-walk-over-vertex)\n", file);
    return;
  }
  const TopoDS_Edge edge = sloped_edge(body, foot);
  check(!edge.IsNull(), "the rib's sloped edge is found");
  if (edge.IsNull()) {
    return;
  }
  const TopoDS_Shape result = rounded(body, edge);
  check(!result.IsNull(), "the rounding near the bend's end builds");
  if (result.IsNull()) {
    return;
  }
  check(BRepCheck_Analyzer(result).IsValid(), "the result is valid");
  Bnd_Box inside, box;
  BRepBndLib::AddOptimal(body, inside, false, false);
  BRepBndLib::AddOptimal(result, box, false, false);
  double x0, y0, z0, x1, y1, z1, a0, b0, c0, a1, b1, c1;
  inside.Get(x0, y0, z0, x1, y1, z1);
  box.Get(a0, b0, c0, a1, b1, c1);
  check(a0 >= x0 - 1e-6 && b0 >= y0 - 1e-6 && c0 >= z0 - 1e-6 && a1 <= x1 + 1e-6 && b1 <= y1 + 1e-6 &&
            c1 <= z1 + 1e-6,
        "the result stays within the body's box");
  const double taken = volume_of(body) - volume_of(result);
  const BRepAdaptor_Curve c(edge);
  const double section = 0.5 * 0.5 * (1.0 - std::atan(1.0)) * (c.LastParameter() - c.FirstParameter());
  std::printf("%s: the rounding takes %.6f mm3 (its section along the edge %.6f)\n", file, taken, section);
  check(taken > 0.0, "the rounding takes material away");
  check(taken < 3.0 * section, "it takes less than three times its section along the edge");
}

}  // namespace

int main() {
  // 1801: the inner rim's corner walks over the rim's end vertex.
  test_body("bend-30.brep", gp_Pnt(-28.090465, 0.490321, 1.1));
  // 1802: the outer edge's walk starts on the rib side's edge with the tube.
  test_body("bend-24.brep", gp_Pnt(-25.901310, 0.452109, 1.1));
  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("ok\n");
  return 0;
}
