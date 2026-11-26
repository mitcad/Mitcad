// SPDX-License-Identifier: MIT
// A loft joined to its mirror image (mitcad#2384, GitHub mitcad/Mitcad#641),
// built as the reporter's design builds it, without the design: Loft1 (F4)
// from a circle of diameter 340 on a plane 315 above XY (Sketch1's c2,
// centred on the z axis) to a circle of diameter 170 on XY centred at
// x = 225 (Sketch2's c1), Tangent Edges Merge; Mirror3 (F21) mirrors the
// body about the YZ plane and joins it. The loft's side is an oblique cone:
// at height z a circle of radius 85 + 85 z / 315 centred at
// x = 225 (1 - z / 315). The two sides cross in the plane x = 0, along a
// curve from the top circle's points (0, -170, 315) and (0, 170, 315),
// where the two sides share their tangent plane, down to the crotch at
// (0, 0, 142.26). The kernel's section of the two sides ended 4.2
// micrometres short of (0, -170, 315), so the mirror image's side was not
// cut there and was classified inside the loft as a whole: the join kept
// the original alone (its volume and box, Mirror3 "ok"), and the reporter's
// Fillet5 found no crotch edge and the Shell no mirrored end. The walk of
// the section stopped there and was not put on the top boundary, its room
// for that a thousandth of a parameter where the side's v runs over 387
// (arc length); OCCT port patch 2384 scales that room with a range longer
// than a full turn, and the section reaches the boundary. A union that
// does not reach as far as a body it joins now fails in words
// (geometry/src/boolean.cpp, lost_reach) instead of keeping half a body.
//
// Checked: the loft alone and the loft with the design's two extrusions
// (a cylinder of radius 170, 60 high, on its top, and one of radius 85, 15
// low, under its bottom), each joined to its mirror image: one valid solid
// reaching both arms' ends (x from -310 to 310) with the volume of the
// union. Every horizontal section of the loft is a disc centred at x >= 0;
// a point at x >= 0 lies nearer that centre than the mirrored one, so the
// union's half at x >= 0 is the loft's own half there, and the union's
// volume is twice that half's, integrated here over the height.
//
// For the diagnosis, instead of checking: `test_mirror_loft_join --probe`
// prints the loft's side surface (periodic or not, degrees, poles, knots),
// the section of the side with its mirror image and the kernel's own fuse
// of the two (done, errors and warnings, valid, volume, box), for MitCad's
// loft, for the same loft with its side made non-periodic, and for the
// kernel's own through-sections loft of the same circles.
// `test_mirror_loft_join --write <folder>` writes the loft and its mirror
// image as 2384-loft.brep and 2384-loft-mirrored.brep there (the OCCT lab's
// case of port patch 2384).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <exception>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/transform.hpp"

namespace test {
namespace {

namespace g = mitcad::geometry;

constexpr double kHeight = 315.0; // Plane1's distance, d1
constexpr double kTop = 170.0;    // Sketch1's diameter 340, d2
constexpr double kBottom = 85.0;  // Sketch2's diameter 170, d3
constexpr double kShift = 225.0;  // Sketch2's distance, d4
constexpr double kUp = 60.0;      // Extrude7, d5
constexpr double kDown = 15.0;    // Extrude8, d6

// A circle of `radius` centred at (cx, 0) in its sketch: region r{<name>}.
g::Region circle(double cx, double radius, const char* name) {
  g::Region region = g::circle_region(cx, 0, radius);
  region.loops[0].segments[0].name = name;
  region.name = std::string("r{") + name + "}";
  return region;
}

g::LoftSection section(const g::Frame& frame, const g::Region& region) {
  g::LoftSection s;
  s.frame = frame;
  s.region = region;
  return s;
}

// The reporter's Loft1.
g::ShapePtr reporter_loft() {
  g::Frame top;
  top.origin = gp_Pnt(0, 0, kHeight);
  g::Frame bottom;
  g::LoftSpec spec;
  spec.feature = "F4";
  spec.sections = {section(top, circle(0, kTop, "c2")), section(bottom, circle(kShift, kBottom, "c1"))};
  spec.merge_tangent = true;
  return g::loft(spec);
}

// The loft with Extrude7 and Extrude8 joined, as cylinders.
g::ShapePtr reporter_body() {
  g::ShapePtr body = reporter_loft();
  const g::Shape up(BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, kHeight), gp_Dir(0, 0, 1)), kTop, kUp).Shape());
  const g::Shape down(
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(kShift, 0, -kDown), gp_Dir(0, 0, 1)), kBottom, kDown).Shape());
  for (const g::Shape* extrusion : {&up, &down}) {
    const g::BooleanResult joined = g::boolean(g::BooleanOp::Join, {body.get()}, *extrusion);
    if (joined.pieces.size() != 1) {
      throw std::runtime_error("an extrusion did not join the loft into one solid");
    }
    body = joined.pieces.front().shape;
  }
  return body;
}

// The mirror image about the YZ plane, as Mirror3 makes it.
g::ShapePtr mirrored(const g::Shape& body) {
  g::Affine map;
  map.linear = {{{-1, 0, 0}, {0, 1, 0}, {0, 0, 1}}};
  return g::transform_shape(body, map, "F21:inst1");
}

// The loft's volume: the area of its section at height z integrated over
// the height (Simpson's rule), of the whole section or (`half`) of its part
// at x >= 0.
double loft_volume(bool half) {
  const auto area = [half](double z) {
    const double centre = kShift * (1.0 - z / kHeight);
    const double radius = kBottom + (kTop - kBottom) * z / kHeight;
    const double disc = kPi * radius * radius;
    if (!half || centre >= radius) {
      return disc;
    }
    // Less the segment beyond the chord at x = 0, `centre` from the centre.
    return disc - (radius * radius * std::acos(centre / radius) - centre * std::sqrt(radius * radius - centre * centre));
  };
  const int steps = 100000;
  double sum = area(0.0) + area(kHeight);
  for (int i = 1; i < steps; ++i) {
    sum += (i % 2 == 1 ? 4.0 : 2.0) * area(kHeight * i / steps);
  }
  return sum * kHeight / steps / 3.0;
}

bool valid(const TopoDS_Shape& shape) { return BRepCheck_Analyzer(shape).IsValid(); }

// The body joined to its mirror image: one valid solid holding both arms
// with `expected` volume.
void check_joined(const char* what, const g::ShapePtr& body, double expected, double low, double high) {
  const g::ShapePtr image = mirrored(*body);
  const g::BooleanResult joined = g::boolean(g::BooleanOp::Join, {body.get()}, *image);
  CHECK(joined.pieces.size() == 1);
  if (joined.pieces.size() != 1) {
    std::fprintf(stderr, "%s: %zu pieces\n", what, joined.pieces.size());
    return;
  }
  const g::Shape& result = *joined.pieces.front().shape;
  CHECK(valid(result.occt()));
  const g::BoundingBox box = g::bounding_box(result);
  std::printf("%s: volume %.3f (expected %.3f), x %.3f .. %.3f, z %.3f .. %.3f\n", what, g::volume(result),
              expected, box.min.X(), box.max.X(), box.min.Z(), box.max.Z());
  CHECK_NEAR_TOL(g::volume(result), expected, 1e-5);
  CHECK(std::abs(box.min.X() + kShift + kBottom) < 1e-3);
  CHECK(std::abs(box.max.X() - kShift - kBottom) < 1e-3);
  CHECK(std::abs(box.min.Z() - low) < 1e-3);
  CHECK(std::abs(box.max.Z() - high) < 1e-3);
}

void test_loft_joined_to_its_mirror() {
  const g::ShapePtr loft = reporter_loft();
  CHECK(valid(loft->occt()));
  CHECK_NEAR(g::volume(*loft), loft_volume(false));
  check_joined("the loft and its mirror image", loft, 2.0 * loft_volume(true), 0.0, kHeight);
}

void test_reporter_body_joined_to_its_mirror() {
  // The extrusions: the top one lies on its mirror image, the bottom one
  // is apart from its own.
  const double up = kPi * kTop * kTop * kUp;
  const double down = kPi * kBottom * kBottom * kDown;
  check_joined("the loft with its extrusions and its mirror image", reporter_body(),
               2.0 * loft_volume(true) + up + 2.0 * down, -kDown, kHeight + kUp);
}

// --- The probe --------------------------------------------------------------

// The shape's first face that is not planar: the loft's side.
TopoDS_Face side_of(const TopoDS_Shape& shape) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face face = TopoDS::Face(it.Current());
    if (BRepAdaptor_Surface(face, false).GetType() != GeomAbs_Plane) {
      return face;
    }
  }
  return TopoDS_Face();
}

void print_surface(const char* what, const TopoDS_Face& face) {
  if (face.IsNull()) {
    std::printf("%s: no side face\n", what);
    return;
  }
  const occ::handle<Geom_BSplineSurface> spline = occ::down_cast<Geom_BSplineSurface>(BRep_Tool::Surface(face));
  if (spline.IsNull()) {
    std::printf("%s: side surface type %d (not a B-spline)\n", what,
                static_cast<int>(BRepAdaptor_Surface(face, false).GetType()));
    return;
  }
  double u1 = 0.0, u2 = 0.0, v1 = 0.0, v2 = 0.0;
  spline->Bounds(u1, u2, v1, v2);
  std::printf("%s: B-spline, U periodic %d, V periodic %d, degrees %d x %d, poles %d x %d, rational %d/%d, "
              "u %.9g .. %.9g, v %.9g .. %.9g\n",
              what, spline->IsUPeriodic() ? 1 : 0, spline->IsVPeriodic() ? 1 : 0, spline->UDegree(),
              spline->VDegree(), spline->NbUPoles(), spline->NbVPoles(), spline->IsURational() ? 1 : 0,
              spline->IsVRational() ? 1 : 0, u1, u2, v1, v2);
  std::printf("  U knots:");
  for (int i = 1; i <= spline->NbUKnots(); ++i) {
    std::printf(" %.9g(%d)", spline->UKnot(i), spline->UMultiplicity(i));
  }
  std::printf("\n  V knots:");
  for (int i = 1; i <= spline->NbVKnots(); ++i) {
    std::printf(" %.9g(%d)", spline->VKnot(i), spline->VMultiplicity(i));
  }
  double fu1 = 0.0, fu2 = 0.0, fv1 = 0.0, fv2 = 0.0;
  BRepTools::UVBounds(face, fu1, fu2, fv1, fv2);
  std::printf("\n  face u %.9g .. %.9g, v %.9g .. %.9g, orientation %d\n", fu1, fu2, fv1, fv2,
              static_cast<int>(face.Orientation()));
}

void print_section(const char* what, const TopoDS_Shape& a, const TopoDS_Shape& b) {
  try {
    BRepAlgoAPI_Section section(a, b, false);
    section.Approximation(true);
    section.Build();
    if (!section.IsDone()) {
      std::printf("%s: section not done\n", what);
      return;
    }
    int edges = 0;
    double length = 0.0;
    for (TopExp_Explorer it(section.Shape(), TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge edge = TopoDS::Edge(it.Current());
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      const BRepAdaptor_Curve curve(edge);
      const double l = GCPnts_AbscissaPoint::Length(curve);
      const gp_Pnt s = curve.Value(curve.FirstParameter());
      const gp_Pnt e = curve.Value(curve.LastParameter());
      std::printf("  %s edge %d: length %.6f, from (%.4f, %.4f, %.4f) to (%.4f, %.4f, %.4f)\n", what, ++edges, l,
                  s.X(), s.Y(), s.Z(), e.X(), e.Y(), e.Z());
      length += l;
    }
    std::printf("%s: %d section edges, length %.6f%s\n", what, edges, length,
                section.HasWarnings() ? " (warnings)" : "");
  } catch (const Standard_Failure& failure) {
    std::printf("%s: section raised %s\n", what, failure.what());
  }
}

void print_fuse(const char* what, const TopoDS_Shape& a, const TopoDS_Shape& b, double fuzzy = 0.0) {
  try {
    BRepAlgoAPI_Fuse fuse;
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(a);
    NCollection_List<TopoDS_Shape> tools;
    tools.Append(b);
    fuse.SetArguments(arguments);
    fuse.SetTools(tools);
    fuse.SetFuzzyValue(fuzzy);
    fuse.Build();
    std::printf("%s (fuzzy %g): fuse done %d, errors %d, warnings %d\n", what, fuzzy, fuse.IsDone() ? 1 : 0,
                fuse.HasErrors() ? 1 : 0, fuse.HasWarnings() ? 1 : 0);
    if (fuse.HasErrors()) {
      fuse.DumpErrors(std::cout);
    }
    if (fuse.HasWarnings()) {
      fuse.DumpWarnings(std::cout);
    }
    std::cout.flush();
    if (!fuse.IsDone()) {
      return;
    }
    const TopoDS_Shape result = fuse.Shape();
    GProp_GProps props;
    BRepGProp::VolumeProperties(result, props);
    Bnd_Box box;
    BRepBndLib::AddOptimal(result, box, false, false);
    int solids = 0;
    int faces = 0;
    for (TopExp_Explorer it(result, TopAbs_SOLID); it.More(); it.Next()) {
      ++solids;
    }
    for (TopExp_Explorer it(result, TopAbs_FACE); it.More(); it.Next()) {
      ++faces;
    }
    std::printf("%s: %d solids, %d faces, valid %d, volume %.3f, x %.3f .. %.3f\n", what, solids, faces,
                valid(result) ? 1 : 0, props.Mass(), box.IsVoid() ? 0.0 : box.CornerMin().X(),
                box.IsVoid() ? 0.0 : box.CornerMax().X());
  } catch (const Standard_Failure& failure) {
    std::printf("%s: fuse raised %s\n", what, failure.what());
  }
}

TopoDS_Shape mirror_occt(const TopoDS_Shape& shape) {
  gp_Trsf trsf;
  trsf.SetMirror(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)));
  return BRepBuilderAPI_Transform(shape, trsf, true).Shape();
}

// A copy of the shape with each U-periodic B-spline face's surface made
// clamped (the same parameters), its edges' curves on it carried over.
TopoDS_Shape clamped(const TopoDS_Shape& shape) {
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(shape).Shape();
  BRep_Builder builder;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapes(copy, TopAbs_FACE, faces);
  for (int i = 1; i <= faces.Extent(); ++i) {
    const TopoDS_Face face = TopoDS::Face(faces(i).Oriented(TopAbs_FORWARD));
    TopLoc_Location location;
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
    const occ::handle<Geom_BSplineSurface> spline = occ::down_cast<Geom_BSplineSurface>(surface);
    if (spline.IsNull() || !spline->IsUPeriodic()) {
      continue;
    }
    const occ::handle<Geom_BSplineSurface> plain = occ::down_cast<Geom_BSplineSurface>(spline->Copy());
    plain->SetUNotPeriodic();
    struct OnFace {
      TopoDS_Edge edge;
      occ::handle<Geom2d_Curve> forward;
      occ::handle<Geom2d_Curve> reversed;
    };
    std::vector<OnFace> curves;
    for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
      OnFace on;
      on.edge = TopoDS::Edge(it.Current().Oriented(TopAbs_FORWARD));
      double first = 0.0;
      double last = 0.0;
      on.forward = BRep_Tool::CurveOnSurface(on.edge, face, first, last);
      if (BRep_Tool::IsClosed(on.edge, face)) {
        on.reversed = BRep_Tool::CurveOnSurface(TopoDS::Edge(on.edge.Oriented(TopAbs_REVERSED)), face, first, last);
      }
      curves.push_back(on);
    }
    builder.UpdateFace(face, plain, location, BRep_Tool::Tolerance(face));
    for (const OnFace& on : curves) {
      const double tolerance = BRep_Tool::Tolerance(on.edge);
      builder.UpdateEdge(on.edge, occ::handle<Geom2d_Curve>(), surface, location, tolerance);
      if (on.reversed.IsNull()) {
        builder.UpdateEdge(on.edge, on.forward, face, tolerance);
      } else {
        builder.UpdateEdge(on.edge, on.forward, on.reversed, face, tolerance);
      }
    }
  }
  return copy;
}

// The kernel's own loft through the same circles.
TopoDS_Shape occt_loft() {
  BRepOffsetAPI_ThruSections maker(true, false);
  const gp_Circ top(gp_Ax2(gp_Pnt(0, 0, kHeight), gp_Dir(0, 0, 1)), kTop);
  const gp_Circ bottom(gp_Ax2(gp_Pnt(kShift, 0, 0), gp_Dir(0, 0, 1)), kBottom);
  maker.AddWire(BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(top).Edge()).Wire());
  maker.AddWire(BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(bottom).Edge()).Wire());
  maker.Build();
  return maker.Shape();
}

void probe_one(const char* what, const TopoDS_Shape& body) {
  const TopoDS_Shape image = mirror_occt(body);
  std::printf("== %s: valid %d\n", what, valid(body) ? 1 : 0);
  print_surface("side", side_of(body));
  print_surface("mirrored side", side_of(image));
  print_section("sides", side_of(body), side_of(image));
  print_section("bodies", body, image);
  for (const double fuzzy : {0.0, 1.0e-4, 1.0e-3, 3.0e-3, 1.0e-2}) {
    print_fuse("kernel fuse", body, image, fuzzy);
  }
  std::fflush(stdout);
}

void probe() {
  std::printf("expected union of the loft %.3f, loft %.3f\n", 2.0 * loft_volume(true), loft_volume(false));
  const g::ShapePtr loft = reporter_loft();
  probe_one("MitCad's loft", loft->occt());
  try {
    const g::ShapePtr image = mirrored(*loft);
    const g::BooleanResult joined = g::boolean(g::BooleanOp::Join, {loft.get()}, *image);
    for (const g::BooleanPiece& piece : joined.pieces) {
      const g::BoundingBox box = g::bounding_box(*piece.shape);
      std::printf("MitCad's join: piece of %zu sources, volume %.3f, x %.3f .. %.3f\n", piece.sources.size(),
                  g::volume(*piece.shape), box.min.X(), box.max.X());
    }
  } catch (const std::exception& error) {
    std::printf("MitCad's join raised %s\n", error.what());
  }
  try {
    probe_one("MitCad's loft, its side clamped", clamped(loft->occt()));
  } catch (const Standard_Failure& failure) {
    std::printf("clamped copy raised %s\n", failure.what());
  }
  try {
    probe_one("the kernel's through-sections loft", occt_loft());
  } catch (const Standard_Failure& failure) {
    std::printf("through-sections loft raised %s\n", failure.what());
  }
}

} // namespace
} // namespace test

int main(int argc, char** argv) {
  if (argc > 1 && std::string(argv[1]) == "--probe") {
    test::probe();
    return 0;
  }
  if (argc > 2 && std::string(argv[1]) == "--write") {
    // The OCCT lab's case (port patch 2384): the loft and its mirror image.
    const mitcad::geometry::ShapePtr loft = test::reporter_loft();
    const std::string folder = argv[2];
    const bool written = BRepTools::Write(loft->occt(), (folder + "/2384-loft.brep").c_str()) &&
                         BRepTools::Write(test::mirrored(*loft)->occt(), (folder + "/2384-loft-mirrored.brep").c_str());
    std::printf("%s\n", written ? "written" : "not written");
    return written ? 0 : 1;
  }
  test::guarded("test_loft_joined_to_its_mirror", test::test_loft_joined_to_its_mirror);
  test::guarded("test_reporter_body_joined_to_its_mirror", test::test_reporter_body_joined_to_its_mirror);
  if (test::failures == 0) {
    std::puts("test_mirror_loft_join: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
