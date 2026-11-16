// SPDX-License-Identifier: MIT
// A fin's foot rounded higher than the flat stretch its curved side bends
// into, with OCCT's fillet alone (mitcad#1926, the port's patch 1700).
//
// The case `fillet-wall-run-on` of MITCAD_GEOMETRY_CASES: `body.brep`, a
// reported design's body before its rounding (GitHub #371's design, the
// state before Fillet1). A U-shaped fin with arms 2 thick stands on a plate
// (z = -547); its top is a plane sloping at 30 degrees that runs down to
// the plate at the arms' ends (y = 435.417), and the fin is extruded from
// its top along the top's normal, so that its sides lean. Each arm's outer
// side is a flat stretch (a plane, at most 2.676 high) that a curved side
// (an extrusion of a B-spline curve) bends into tangentially. The outer
// foot, a chain of eight edges from one arm's end round the fin to the
// other's, is rounded with 3.
//
// The ball's contact leaves the curved side across its edge with the top,
// the face at the chain's end, and the blend runs on: over the curved
// side's surface continued, and past the flat stretch's edge over that
// stretch's plane, up to where the contact on the plate meets the top's
// line with the plate continued past the arm's end; the top cuts it there
// and grows out to meet it, and the flat stretch is gone. Before the patch
// the run-on went over the B-spline curve's extrapolation, which bends
// away from the plane, the walk stopped short of the end and the fillet
// failed.
//
// Checks: the fillet builds; the result is valid; the flat stretches are
// gone and the top grew out over the rounding; the volume is that of the
// same rounding of the fin raised along its top's normal (its sides
// continued), less the raise and what stands past the top's plane, within
// a thousandth of the material the rounding adds (the walked blend's
// surface is approximated).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

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

const double kRadius = 3.0;
const double kPlate = -547.0;
// The arms' outer sides (the flat stretches) and the top's outward normal.
const double kFlatX[2] = {-273.431, -289.931};
const gp_Dir kTopNormal(0.0, 0.5, std::sqrt(3.0) / 2.0);

// The outer foot: the edges on the plate's top round the fin's outer
// outline (not across an arm's end, not along the arms' inner sides and
// the space between them, x -287.9 to -275.4 with y above 422.5).
std::vector<TopoDS_Edge> outer_foot(const TopoDS_Shape& body) {
  std::vector<TopoDS_Edge> edges;
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndAncestors(body, TopAbs_EDGE, TopAbs_FACE, faces_of);
  for (int i = 1; i <= faces_of.Extent(); ++i) {
    const TopoDS_Edge& e = TopoDS::Edge(faces_of.FindKey(i));
    if (faces_of(i).Extent() != 2 || BRep_Tool::Degenerated(e)) {
      continue;
    }
    const BRepAdaptor_Curve c(e);
    const gp_Pnt a = c.Value(c.FirstParameter());
    const gp_Pnt b = c.Value(c.LastParameter());
    const gp_Pnt m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    if (std::abs(a.Z() - kPlate) > 1e-6 || std::abs(b.Z() - kPlate) > 1e-6 || std::abs(m.Z() - kPlate) > 1e-6 ||
        m.X() < -290.0 || m.X() > -273.4 || m.Y() < 418.0 || m.Y() > 435.4) {
      continue;
    }
    if (std::abs(a.Y() - b.Y()) < 1e-6 && std::abs(a.Y() - 435.417) < 1e-3) {
      continue;
    }
    if (m.X() > -287.94 && m.X() < -275.42 && m.Y() > 422.5) {
      continue;
    }
    edges.push_back(e);
  }
  return edges;
}

// The fin's top: the plane with the top's normal.
TopoDS_Face top_of(const TopoDS_Shape& body) {
  for (TopExp_Explorer ex(body, TopAbs_FACE); ex.More(); ex.Next()) {
    const BRepAdaptor_Surface s(TopoDS::Face(ex.Current()));
    if (s.GetType() == GeomAbs_Plane && s.Plane().Axis().Direction().IsParallel(kTopNormal, 1e-6)) {
      return TopoDS::Face(ex.Current());
    }
  }
  return TopoDS_Face();
}

// The same rounding of the fin raised along its top's normal (its sides
// continued), less the raise and what stands past the top's plane.
double reference_volume(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& foot) {
  const TopoDS_Face top = top_of(body);
  if (top.IsNull()) {
    return -1.0;
  }
  const TopoDS_Shape prism = BRepPrimAPI_MakePrism(top, gp_Vec(kTopNormal) * 5.0).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(body, prism).Shape(), true, true, true);
  unify.Build();
  const TopoDS_Shape raised = unify.Shape();
  BRepFilletAPI_MakeFillet maker(raised);
  for (TopExp_Explorer ex(raised, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    const BRepAdaptor_Curve c(e);
    const gp_Pnt m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    for (const TopoDS_Edge& f : foot) {
      const BRepAdaptor_Curve cf(f);
      if (cf.Value(0.5 * (cf.FirstParameter() + cf.LastParameter())).Distance(m) < 1e-6 && maker.Contour(e) == 0) {
        maker.Add(kRadius, e);
      }
    }
  }
  maker.Build();
  if (!maker.IsDone()) {
    return -1.0;
  }
  const TopoDS_Shape added = BRepAlgoAPI_Cut(maker.Shape(), raised).Shape();
  const gp_Pnt at = BRepAdaptor_Surface(top).Plane().Location();
  const TopoDS_Face plane = BRepBuilderAPI_MakeFace(gp_Pln(at, kTopNormal), -1000, 1000, -1000, 1000).Face();
  const TopoDS_Shape beyond = BRepPrimAPI_MakeHalfSpace(plane, at.Translated(gp_Vec(kTopNormal))).Solid();
  return volume_of(maker.Shape()) - volume_of(prism) - volume_of(BRepAlgoAPI_Common(added, beyond).Shape());
}

void test_reported_body() {
  const std::string dir = cases_dir();
  if (dir.empty()) {
    std::printf("test_reported_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  TopoDS_Shape body;
  BRep_Builder builder;
  if (!BRepTools::Read(body, (dir + "/fillet-wall-run-on/body.brep").c_str(), builder) || body.IsNull()) {
    std::printf("test_reported_body: skipped (no case fillet-wall-run-on)\n");
    return;
  }
  const std::vector<TopoDS_Edge> foot = outer_foot(body);
  check(foot.size() == 8, "the outer foot has eight edges");
  BRepFilletAPI_MakeFillet maker(body);
  for (const TopoDS_Edge& e : foot) {
    if (maker.Contour(e) == 0) {
      maker.Add(kRadius, e);
    }
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "the fillet throws: %s\n", e.what());
  }
  check(maker.IsDone(), "the fillet builds");
  if (!maker.IsDone()) {
    return;
  }
  const TopoDS_Shape& rounded = maker.Shape();
  check(BRepCheck_Analyzer(rounded).IsValid(), "the result is valid");
  // The flat stretches are gone; the top grew out over the rounding.
  bool flat = false;
  double top_x0 = 0.0;
  double top_x1 = -1e9;
  for (TopExp_Explorer ex(rounded, TopAbs_FACE); ex.More(); ex.Next()) {
    const BRepAdaptor_Surface s(TopoDS::Face(ex.Current()));
    if (s.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln p = s.Plane();
    Bnd_Box box;
    BRepBndLib::Add(ex.Current(), box);
    double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
    box.Get(x0, y0, z0, x1, y1, z1);
    for (const double x : kFlatX) {
      if (p.Axis().Direction().IsParallel(gp_Dir(1, 0, 0), 1e-9) && p.Distance(gp_Pnt(x, 0, 0)) < 1e-3 &&
          y0 > 429.0 && z0 > kPlate - 1e-3) {
        flat = true;
      }
    }
    if (p.Axis().Direction().IsParallel(kTopNormal, 1e-6)) {
      top_x0 = std::min(top_x0, x0);
      top_x1 = std::max(top_x1, x1);
    }
  }
  check(!flat, "the flat stretches are gone");
  // Every face of the result is an input face (kept or modified) or a face
  // the history gives as generated by an edge of the foot: the pieces the
  // end's walk on over the flat stretch made too, which a later rounding
  // along the top's edge finds by their names (mitcad#1926).
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> known;
  for (TopExp_Explorer ex(body, TopAbs_FACE); ex.More(); ex.Next()) {
    known.Add(ex.Current());
    for (const TopoDS_Shape& image : maker.Modified(ex.Current())) {
      known.Add(image);
    }
  }
  for (const TopoDS_Edge& e : foot) {
    for (const TopoDS_Shape& made : maker.Generated(e)) {
      known.Add(made);
    }
  }
  int unknown = 0;
  for (TopExp_Explorer ex(rounded, TopAbs_FACE); ex.More(); ex.Next()) {
    if (!known.Contains(ex.Current())) {
      ++unknown;
    }
  }
  check(unknown == 0, "every face of the rounding is generated by an edge of the foot");
  if (unknown != 0) {
    std::fprintf(stderr, "%d faces neither the input's nor generated by the foot's edges\n", unknown);
  }
  check(top_x1 > kFlatX[0] + 2.5 && top_x0 < kFlatX[1] - 2.5, "the top grew out over the rounding");
  const double expected = reference_volume(body, foot);
  const double got = volume_of(rounded);
  const double added = expected - volume_of(body);
  const bool near = expected > 0.0 && std::abs(got - expected) <= 1e-3 * added;
  check(near, "the volume is the reference's");
  if (!near) {
    std::fprintf(stderr, "volume %.9g, expected %.9g (added %.9g)\n", got, expected, added);
  }
}

} // namespace

int main() {
  try {
    test_reported_body();
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "unexpected OCCT error: %s\n", e.what());
    ++failures;
  }
  if (failures > 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("test_fillet_wall_run_on: all checks passed\n");
  return 0;
}
