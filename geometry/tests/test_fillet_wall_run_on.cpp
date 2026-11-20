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
// the face at the chain's end, and the blend runs on over the curved
// side's surface continued past the flat stretch's edge by the polynomial
// of its curve's last span, which bends in from the stretch's plane as the
// curve went on bending (mitcad#1987: so the bodies stored in the reported
// design have it; over the stretch's plane, the patch's first form, the
// foot lay up to 0.45 mm further out at the arm's end). It runs up to
// where the contact on the plate meets the top's line with the plate
// continued past the arm's end; the top cuts it there and grows out to
// meet it, and the flat stretch is gone. Before the patch the walk stopped
// at the curved side's bounds and the fillet failed.
//
// Checks: the fillet builds; the result is valid; the flat stretches are
// gone and the top grew out over the rounding; the rounding's foot on the
// plate along each flat stretch is that of a ball on the plate and on the
// curved side's surface so continued, within 0.01 mm, and turns in by more
// than 0.3 mm at the arm's end; the volume is less than that of the same
// rounding of the fin raised along its top's normal (its sides continued
// flat), less the raise and what stands past the top's plane, and not by
// more than a twentieth of the material the rounding adds.

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
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomConvert_BSplineCurveToBezierCurve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_SurfaceOfLinearExtrusion.hxx>
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
#include <TopoDS_Compound.hxx>
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

// The foot on the plate, along the flat stretch at kFlatX[k], of a ball of
// kRadius on the plate and on the curved side bending into that stretch,
// its surface continued past the stretch's edge by the polynomial of its
// curve's span there: points with y from 431.5 to 435.2 (the arm's end at
// 435.417). Empty where no such curved side is found.
std::vector<gp_Pnt> expected_foot(const TopoDS_Shape& body, int k) {
  std::vector<gp_Pnt> out;
  const double outward = k == 1 ? -1.0 : 1.0;
  for (TopExp_Explorer ex(body, TopAbs_FACE); ex.More() && out.empty(); ex.Next()) {
    Handle(Geom_Surface) s = BRep_Tool::Surface(TopoDS::Face(ex.Current()));
    if (const Handle(Geom_RectangularTrimmedSurface) t = Handle(Geom_RectangularTrimmedSurface)::DownCast(s)) {
      s = t->BasisSurface();
    }
    const Handle(Geom_SurfaceOfLinearExtrusion) side = Handle(Geom_SurfaceOfLinearExtrusion)::DownCast(s);
    if (side.IsNull()) {
      continue;
    }
    const Handle(Geom_BSplineCurve) curve = Handle(Geom_BSplineCurve)::DownCast(side->BasisCurve());
    if (curve.IsNull() || curve->IsRational()) {
      continue;
    }
    // The end of the curve at the flat stretch's edge.
    const bool at_last = std::abs(curve->EndPoint().X() - kFlatX[k]) < 1e-3;
    if (!at_last && std::abs(curve->StartPoint().X() - kFlatX[k]) >= 1e-3) {
      continue;
    }
    GeomConvert_BSplineCurveToBezierCurve arcs(curve);
    const Handle(Geom_BezierCurve) span = arcs.Arc(at_last ? arcs.NbArcs() : 1);
    // Its span continued by its polynomial, a span's length further on.
    span->Segment(at_last ? 1.0 : -1.0, at_last ? 2.0 : 0.0);
    const gp_Vec dir(side->Direction());
    for (int i = 0; i <= 400; ++i) {
      gp_Pnt q;
      gp_Vec t;
      span->D1(i / 400.0, q, t);
      gp_Vec n = t.Crossed(dir);
      if (n.Magnitude() < 1e-9) {
        continue;
      }
      n.Normalize();
      if (n.X() * outward < 0.0) {
        n.Reverse();
      }
      const double v = (kPlate + kRadius - kRadius * n.Z() - q.Z()) / dir.Z();
      const gp_Pnt centre = q.Translated(dir * v + n * kRadius);
      if (centre.Y() >= 431.5 && centre.Y() <= 435.2) {
        out.emplace_back(centre.X(), centre.Y(), kPlate);
      }
    }
  }
  return out;
}

// The rounding's foot on the plate along the flat stretch at kFlatX[k]
// matches expected_foot within 0.01 mm and turns in by more than 0.3 mm at
// the arm's end.
void check_foot(const TopoDS_Shape& body, const TopoDS_Shape& rounded, int k) {
  const std::vector<gp_Pnt> want = expected_foot(body, k);
  check(want.size() > 20, "the curved side bending into the flat stretch is found");
  if (want.size() <= 20) {
    return;
  }
  const double outward = k == 1 ? -1.0 : 1.0;
  const double straight = kFlatX[k] + outward * kRadius;
  const gp_Pnt end = *std::max_element(want.begin(), want.end(),
                                       [](const gp_Pnt& a, const gp_Pnt& b) { return a.Y() < b.Y(); });
  const double turned_in = std::abs(end.X() - straight);
  check(turned_in > 0.3, "the continued surface turns the foot in at the arm's end");
  TopoDS_Compound feet;
  BRep_Builder builder;
  builder.MakeCompound(feet);
  int count = 0;
  for (TopExp_Explorer ex(rounded, TopAbs_EDGE); ex.More(); ex.Next()) {
    const BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
    const gp_Pnt a = c.Value(c.FirstParameter());
    const gp_Pnt b = c.Value(c.LastParameter());
    const gp_Pnt m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    if (std::abs(a.Z() - kPlate) < 1e-6 && std::abs(b.Z() - kPlate) < 1e-6 && std::abs(m.Z() - kPlate) < 1e-6 &&
        std::abs(m.X() - straight) < 1.0 && m.Y() > 429.0 && m.Y() < 435.5) {
      builder.Add(feet, ex.Current());
      ++count;
    }
  }
  check(count > 0, "the rounding has a foot along the flat stretch");
  if (count == 0) {
    return;
  }
  double worst = 0.0;
  for (const gp_Pnt& p : want) {
    BRepExtrema_DistShapeShape dist(BRepBuilderAPI_MakeVertex(p).Vertex(), feet);
    worst = std::max(worst, dist.IsDone() ? dist.Value() : 1e9);
  }
  check(worst <= 0.01, "the foot is the ball's on the curved side continued");
  std::printf("arm at x %.3f: foot turned in %.4f mm at the arm's end, farthest %.5f mm from the expected foot\n",
              kFlatX[k], turned_in, worst);
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
  check(top_x1 > kFlatX[0] + 2.0 && top_x0 < kFlatX[1] - 2.0, "the top grew out over the rounding");
  check_foot(body, rounded, 0);
  check_foot(body, rounded, 1);
  // The raised reference runs the rounding on over the flat stretches'
  // planes: an upper bound.
  const double flat_volume = reference_volume(body, foot);
  const double got = volume_of(rounded);
  const double added = flat_volume - volume_of(body);
  const bool near = flat_volume > 0.0 && got < flat_volume && flat_volume - got <= 0.05 * added;
  check(near, "the volume is a little under the flat run-on's");
  std::printf("volume %.9g, flat run-on %.9g (added %.9g)\n", got, flat_volume, added);
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
