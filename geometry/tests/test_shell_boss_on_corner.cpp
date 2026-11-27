// SPDX-License-Identifier: MIT
// Shells and closed offsets of a plate with a cylinder standing on the axis
// of one of its rounded corners, of the corner's radius (GitHub #466,
// mitcad#2050), with OCCT alone. The cylinder continues a quarter of the
// corner's side (one face after the join), so the concave edge round the
// cylinder's foot runs three quarters round, and the plate's top ends in a
// point of no angle at each of its ends (the top's straight edge along the
// plate's side and the concave arc leave the point in the same direction,
// the side meeting the cylinder tangentially below it). The offsets of the
// top and the cylinder cut each other in a whole circle, which the offset
// trimmed from its origin to the highest vertex on it: over the corner
// quarter instead of the three quarters across the origin. The shells and
// the closed offset were not done ("the offset faces do not close up"),
// with the foot's faces apart not valid. With the port's patch 2050 such an
// intersection all round a periodic curve is trimmed to the arcs over the
// edges it is the image of, and the top's line and circle touching at the
// point of no angle meet where each crosses the offset of the tangential
// edge below (as patch 1160 does for curves that are not lines or conics).
//
// The plate is 75 x 50 x 10 with corners rounded 15, the cylinder of
// radius 15 and 10 high on the corner about (-22.5, -10), as the
// reporter's design has them (its sketch solved the radius to
// 14.999999999999996 and the centre 4e-15 off: the second case). The
// shells remove the cylinder's top and the plate's bottom; their volumes
// follow from the areas of the rounded rectangles and circles.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <sstream>
#include <string>
#include <typeinfo>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_IndexedMap.hxx>
#include <BRepOffset_Analyse.hxx>
#include <BRepOffset_Interval.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <ChFiDS_TypeOfConcavity.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <BRep_Tool.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>

#include "check.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/shape.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

const gp_Dir kZ(0, 0, 1);
const gp_Dir kX(1, 0, 0);

// The plate's half sizes, the corners' radius and height, the cylinder's.
constexpr double kHalfX = 37.5;
constexpr double kHalfY = 25.0;
constexpr double kCorner = 15.0;
constexpr double kPlate = 10.0;
constexpr double kBoss = 10.0;

double volume_of(const TopoDS_Shape& shape) {
  return mitcad::geometry::mass_properties(mitcad::geometry::Shape(shape)).volume;
}

// The area of a w x h rectangle with corners rounded r.
double rounded_area(double w, double h, double r) { return w * h - (4.0 - kPi) * r * r; }

// The arc of the circle about `centre` (radius r, its origin towards
// `origin`) from a to b, turning
// about +z.
TopoDS_Edge arc(const gp_Pnt& centre, double r, const gp_Pnt& a, const gp_Pnt& b, const gp_Dir& origin = kX) {
  const gp_Circ circle(gp_Ax2(centre, kZ, origin), r);
  return BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(circle, a, b, true).Value()).Edge();
}

// The plate (corners rounded `corner`) joined with the cylinder (radius
// `radius` about `axis` in the plane of the plate's top), the faces on one
// surface merged as MitCad's join merges them (mitcad#1261) unless `merge`
// is false. With `seam_at_corner` the corner's arc and the cylinder start
// towards -x, so that the merged face's seam runs up from where the
// cylinder meets the plate's side x = -37.5 tangentially, as in the
// reporter's design (its corner's arc starts there); else towards +x.
TopoDS_Shape plate_with_boss(const gp_Pnt& axis, double radius, double corner = kCorner, bool merge = true,
                             bool seam_at_corner = false) {
  const gp_Dir origin = seam_at_corner ? gp_Dir(-1, 0, 0) : kX;
  const double cx = kHalfX - corner, cy = kHalfY - corner;
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-cx, -kHalfY, 0), gp_Pnt(cx, -kHalfY, 0)).Edge());
  wire.Add(arc(gp_Pnt(cx, -cy, 0), corner, gp_Pnt(cx, -kHalfY, 0), gp_Pnt(kHalfX, -cy, 0)));
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(kHalfX, -cy, 0), gp_Pnt(kHalfX, cy, 0)).Edge());
  wire.Add(arc(gp_Pnt(cx, cy, 0), corner, gp_Pnt(kHalfX, cy, 0), gp_Pnt(cx, kHalfY, 0)));
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(cx, kHalfY, 0), gp_Pnt(-cx, kHalfY, 0)).Edge());
  wire.Add(arc(gp_Pnt(-cx, cy, 0), corner, gp_Pnt(-cx, kHalfY, 0), gp_Pnt(-kHalfX, cy, 0)));
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-kHalfX, cy, 0), gp_Pnt(-kHalfX, -cy, 0)).Edge());
  wire.Add(arc(gp_Pnt(-cx, -cy, 0), corner, gp_Pnt(-kHalfX, -cy, 0), gp_Pnt(-cx, -kHalfY, 0), origin));
  CHECK(wire.IsDone());
  const TopoDS_Face base = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  const TopoDS_Shape plate = BRepPrimAPI_MakePrism(base, gp_Vec(0, 0, kPlate)).Shape();
  const TopoDS_Shape boss =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(axis.X(), axis.Y(), kPlate), kZ, origin), radius, kBoss).Shape();
  BRepAlgoAPI_Fuse fuse(plate, boss);
  CHECK(fuse.IsDone() && !fuse.HasErrors());
  if (!merge) {
    return fuse.Shape();
  }
  ShapeUpgrade_UnifySameDomain unify(fuse.Shape(), true, true, false);
  unify.Build();
  return unify.Shape();
}

// The planar faces of the shape parallel to the xy plane at height z.
NCollection_List<TopoDS_Shape> flat_faces_at(const TopoDS_Shape& shape, double z) {
  NCollection_List<TopoDS_Shape> faces;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(it.Current()));
    if (surface.GetType() == GeomAbs_Plane && std::abs(std::abs(surface.Plane().Axis().Direction().Z()) - 1.0) < 1e-9 &&
        std::abs(surface.Plane().Location().Z() - z) < 1e-9) {
      faces.Append(it.Current());
    }
  }
  return faces;
}

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

// What OCCT's checker refuses in the shape: each sub-shape with a status
// other than no error (in the contexts it is checked in), where it is, the
// first 20.
void print_invalid(const char* name, const TopoDS_Shape& shape) {
  const BRepCheck_Analyzer analyzer(shape);
  if (analyzer.IsValid()) {
    return;
  }
  const char* const kinds[] = {"compound", "compsolid", "solid", "shell", "face", "wire", "edge", "vertex"};
  int printed = 0;
  for (const TopAbs_ShapeEnum type : {TopAbs_VERTEX, TopAbs_EDGE, TopAbs_WIRE, TopAbs_FACE, TopAbs_SHELL,
                                      TopAbs_SOLID}) {
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> shapes;
    TopExp::MapShapes(shape, type, shapes);
    for (int i = 1; i <= shapes.Extent() && printed < 20; ++i) {
      const occ::handle<BRepCheck_Result>& result = analyzer.Result(shapes(i));
      if (result.IsNull()) {
        continue;
      }
      std::ostringstream out;
      for (const BRepCheck_Status status : result->Status()) {
        if (status != BRepCheck_NoError) {
          BRepCheck::Print(status, out);
        }
      }
      for (result->InitContextIterator(); result->MoreShapeInContext(); result->NextShapeInContext()) {
        for (const BRepCheck_Status status : result->StatusOnShape()) {
          if (status != BRepCheck_NoError) {
            out << "in a " << kinds[result->ContextualShape().ShapeType()] << ": ";
            BRepCheck::Print(status, out);
          }
        }
      }
      if (out.str().empty()) {
        continue;
      }
      Bnd_Box box;
      BRepBndLib::Add(shapes(i), box);
      double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
      if (!box.IsVoid()) {
        box.Get(x0, y0, z0, x1, y1, z1);
      }
      std::string text = out.str();
      std::replace(text.begin(), text.end(), '\n', ' ');
      std::printf("%s: %s %d in (%.3f %.3f %.3f)-(%.3f %.3f %.3f): %s\n", name, kinds[type], i, x0, y0, z0, x1, y1,
                  z1, text.c_str());
      ++printed;
    }
  }
}

// The result is valid, of the volume given (relative 1e-6) and `solids`
// solids (a closed offset comes as its closed shell, no solid), and the body
// as it was.
void check_result(const char* name, const TopoDS_Shape& body, double before, const TopoDS_Shape& result,
                  double volume, int solids = 1) {
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    std::printf("%s: not done\n", name);
    return;
  }
  const bool valid = BRepCheck_Analyzer(result).IsValid();
  const double v = volume_of(result);
  std::printf("%s: valid %d, %d solids, volume %.6f (expected %.6f)\n", name, (int)valid,
              count(result, TopAbs_SOLID), v, volume);
  print_invalid(name, result);
  CHECK(valid);
  CHECK(count(result, TopAbs_SOLID) == solids);
  CHECK_NEAR_TOL(v, volume, 1e-6);
  CHECK_NEAR_TOL(volume_of(body), before, 1e-9);
}

// The shell of the body with the cylinder's top and the plate's bottom
// removed, by `offset` (negative inside), with OCCT alone.
TopoDS_Shape thick_solid(const char* name, const TopoDS_Shape& body, double offset) {
  NCollection_List<TopoDS_Shape> open = flat_faces_at(body, 0.0);
  NCollection_List<TopoDS_Shape> top = flat_faces_at(body, kPlate + kBoss);
  open.Append(top);
  CHECK(open.Extent() == 2);
  BRepOffsetAPI_MakeThickSolid maker;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(body, open, offset, 1.0e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection,
                               false);
  } catch (const Standard_Failure& error) {
    std::printf("%s: %s\n", name, error.what());
  }
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

TopoDS_Shape closed_offset(const char* name, const TopoDS_Shape& body, double offset) {
  BRepOffsetAPI_MakeOffsetShape maker;
  try {
    OCC_CATCH_SIGNALS
    maker.PerformByJoin(body, offset, 1.0e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection, false);
  } catch (const Standard_Failure& error) {
    std::printf("%s: %s\n", name, error.what());
  }
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

void check_body(const char* label, const gp_Pnt& axis, double radius, bool seam_at_corner = false) {
  const TopoDS_Shape body = plate_with_boss(axis, radius, kCorner, true, seam_at_corner);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const double r = kCorner, t = 2.0;
  const double before = volume_of(body);
  const double plate = rounded_area(2 * kHalfX, 2 * kHalfY, r);
  CHECK_NEAR_TOL(before, plate * kPlate + kPi * r * r * kBoss, 1e-6);
  char name[128];

  // Inside: the plate's inside to 2 below its top, the cylinder's from
  // there up through its open top.
  std::snprintf(name, sizeof name, "%s, shell 2 inside", label);
  const double inner = rounded_area(2 * (kHalfX - t), 2 * (kHalfY - t), r - t);
  const double cavity = inner * (kPlate - t) + kPi * (r - t) * (r - t) * (kBoss + t);
  check_result(name, body, before, thick_solid(name, body, -t), before - cavity);

  // Outside: the plate's outside to 2 above its top, the cylinder's from
  // there up to its open top.
  std::snprintf(name, sizeof name, "%s, shell 2 outside", label);
  const double outer = rounded_area(2 * (kHalfX + t), 2 * (kHalfY + t), r + t);
  const double hull = outer * (kPlate + t) + kPi * (r + t) * (r + t) * (kBoss - t);
  check_result(name, body, before, thick_solid(name, body, t), hull - before);

  // The closed inward offset (the cavity Shell falls back on).
  std::snprintf(name, sizeof name, "%s, closed offset 2 inside", label);
  check_result(name, body, before, closed_offset(name, body, -t),
               inner * (kPlate - 2 * t) + kPi * (r - t) * (r - t) * kBoss, 0);
}

// The probe (argument "probe", mitcad#2050's diagnosis; no checks): how
// OCCT's offset analysis sees the body's edges and vertices, and what each
// offset of the body and of variants of it gives.
const char* concavity(ChFiDS_TypeOfConcavity type) {
  switch (type) {
  case ChFiDS_Concave:
    return "concave";
  case ChFiDS_Convex:
    return "convex";
  case ChFiDS_Tangential:
    return "tangential";
  case ChFiDS_FreeBound:
    return "free";
  case ChFiDS_Mixed:
    return "mixed";
  default:
    return "other";
  }
}

void print_analysis(const TopoDS_Shape& body) {
  BRepOffset_Analyse analyse(body, 4.0 * std::asin(1.0e-6));
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edge_faces;
  TopExp::MapShapesAndAncestors(body, TopAbs_EDGE, TopAbs_FACE, edge_faces);
  for (int i = 1; i <= edge_faces.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(edge_faces.FindKey(i));
    std::string types;
    if (!BRep_Tool::Degenerated(edge)) {
      for (const BRepOffset_Interval& interval : analyse.Type(edge)) {
        char part[64];
        std::snprintf(part, sizeof part, " %s[%.4g,%.4g]", concavity(interval.Type()), interval.First(),
                      interval.Last());
        types += part;
      }
    }
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt a = curve.Value(curve.FirstParameter());
    const gp_Pnt m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    const gp_Pnt b = curve.Value(curve.LastParameter());
    std::printf("  edge %d: curve type %d, %d faces%s, from (%.4f %.4f %.4f) via (%.4f %.4f %.4f) to (%.4f %.4f "
                "%.4f)%s%s\n",
                i, (int)curve.GetType(), edge_faces(i).Extent(), BRep_Tool::Degenerated(edge) ? " degenerated" : "",
                a.X(), a.Y(), a.Z(), m.X(), m.Y(), m.Z(), b.X(), b.Y(), b.Z(),
                BRep_Tool::IsClosed(edge, TopoDS::Face(edge_faces(i).First())) ? " seam" : "", types.c_str());
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> vertex_edges;
  TopExp::MapShapesAndAncestors(body, TopAbs_VERTEX, TopAbs_EDGE, vertex_edges);
  for (int i = 1; i <= vertex_edges.Extent(); ++i) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(vertex_edges.FindKey(i)));
    std::string edges;
    NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> seen;
    for (const TopoDS_Shape& edge : vertex_edges(i)) {
      if (seen.Add(edge)) {
        edges += " " + std::to_string(edge_faces.FindIndex(edge));
      }
    }
    std::printf("  vertex %d at (%.4f %.4f %.4f): %d edges:%s\n", i, p.X(), p.Y(), p.Z(), seen.Extent(),
                edges.c_str());
  }
}

void print_offset(const char* label, const TopoDS_Shape& body, double offset, bool thick, GeomAbs_JoinType join) {
  TopoDS_Shape result;
  bool done = false;
  int error = -1;
  try {
    OCC_CATCH_SIGNALS
    if (thick) {
      NCollection_List<TopoDS_Shape> open = flat_faces_at(body, 0.0);
      NCollection_List<TopoDS_Shape> top = flat_faces_at(body, kPlate + kBoss);
      open.Append(top);
      BRepOffsetAPI_MakeThickSolid maker;
      maker.MakeThickSolidByJoin(body, open, offset, 1.0e-6, BRepOffset_Skin, false, false, join, false);
      done = maker.IsDone();
      error = (int)maker.MakeOffset().Error();
      result = done ? maker.Shape() : TopoDS_Shape();
    } else {
      BRepOffsetAPI_MakeOffsetShape maker;
      maker.PerformByJoin(body, offset, 1.0e-6, BRepOffset_Skin, false, false, join, false);
      done = maker.IsDone();
      error = (int)maker.MakeOffset().Error();
      result = done ? maker.Shape() : TopoDS_Shape();
    }
  } catch (const Standard_Failure& failure) {
    std::printf("  %s: raised %s: %s\n", label, typeid(failure).name(), failure.what());
    return;
  }
  if (!done || result.IsNull()) {
    std::printf("  %s: not done, error %d\n", label, error);
    return;
  }
  std::printf("  %s: done, error %d, valid %d, %d solids, %d faces, volume %.6f\n", label, error,
              (int)BRepCheck_Analyzer(result).IsValid(), count(result, TopAbs_SOLID), count(result, TopAbs_FACE),
              volume_of(result));
  print_invalid(label, result);
}

void probe_body(const char* label, const TopoDS_Shape& body, bool analysis) {
  std::printf("%s: valid %d, %d faces, %d edges, volume %.6f\n", label, (int)BRepCheck_Analyzer(body).IsValid(),
              count(body, TopAbs_FACE), count(body, TopAbs_EDGE), volume_of(body));
  if (analysis) {
    print_analysis(body);
  }
  print_offset("shell 2 inside", body, -2.0, true, GeomAbs_Intersection);
  print_offset("shell 1 inside", body, -1.0, true, GeomAbs_Intersection);
  print_offset("shell 2 outside", body, 2.0, true, GeomAbs_Intersection);
  print_offset("closed offset 2 inside", body, -2.0, false, GeomAbs_Intersection);
  print_offset("closed offset 2 outside", body, 2.0, false, GeomAbs_Intersection);
  print_offset("shell 2 inside, arc join", body, -2.0, true, GeomAbs_Arc);
  print_offset("closed offset 2 inside, arc join", body, -2.0, false, GeomAbs_Arc);
}

void probe() {
  const gp_Pnt axis(-22.5, -10.0, 0.0);
  probe_body("A: radius 15 on the corner's axis, merged", plate_with_boss(axis, 15.0), true);
  probe_body("B: radius 15 on the corner's axis, not merged", plate_with_boss(axis, 15.0, kCorner, false), true);
  probe_body("C: radius 10 on the corner's axis", plate_with_boss(axis, 10.0), false);
  probe_body("D: radius 15 on the axis of a corner rounded 20", plate_with_boss(gp_Pnt(-17.5, -5.0, 0.0), 15.0, 20.0),
             false);
  probe_body("E: radius 15 on the axis of a corner rounded 16 (1 mm apart)",
             plate_with_boss(gp_Pnt(-21.5, -9.0, 0.0), 15.0, 16.0), false);
  probe_body("F: radius 15 on the corner's axis, merged, the seam at the corner",
             plate_with_boss(axis, 15.0, kCorner, true, true), true);
}

void test_boss_on_a_rounded_corner() {
  check_body("radius 15", gp_Pnt(-22.5, -10.0, 0.0), 15.0);
  check_body("radius as solved", gp_Pnt(-22.500000000000004, -10.000000000000004, 0.0), 14.999999999999996);
  // The seam of the merged face where the reporter's design has it (mitcad#2050).
  check_body("radius 15, seam at the corner", gp_Pnt(-22.5, -10.0, 0.0), 15.0, true);
}

} // namespace
} // namespace test

// With the argument "probe" the diagnosis's printout instead of the checks;
// with "probe" and a BRep file, the same printout for the body in it (its
// open faces the flat ones at heights 0 and 20).
MITCAD_BUNDLED_TEST_ARGS(shell_boss_on_corner) {
  if (argc > 2 && std::string(argv[1]) == "probe") {
    const std::string path = argv[2];
    test::guarded("probe file", [&path] {
      TopoDS_Shape body;
      BRep_Builder builder;
      CHECK(BRepTools::Read(body, path.c_str(), builder));
      if (!body.IsNull()) {
        test::probe_body(path.c_str(), body, true);
      }
    });
    return test::failures == 0 ? 0 : 1;
  }
  if (argc > 1 && std::string(argv[1]) == "probe") {
    test::guarded("probe", test::probe);
    return test::failures == 0 ? 0 : 1;
  }
  test::guarded("test_boss_on_a_rounded_corner", test::test_boss_on_a_rounded_corner);
  if (test::failures == 0) {
    std::puts("test_shell_boss_on_corner: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
