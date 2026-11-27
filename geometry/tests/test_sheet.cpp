// SPDX-License-Identifier: MIT
// Planar sheet construction with real OCCT: curve transport, named material
// sides, domain membership, invalid contours and cancellation (mitcad#206).

#include <limits>
#include <set>
#include <utility>

#include "check.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "bundled_test.hpp"

namespace {

using namespace mitcad::geometry;
using test::near;
using test::throws_with;
constexpr double kPi = test::kPi;

Region named(Region region, int first = 1) {
  for (Loop& loop : region.loops) {
    for (Segment& segment : loop.segments) {
      segment.name = "F1:sheet_curve(c" + std::to_string(first++) + ")";
    }
  }
  return region;
}

SheetShapeSpec one(Region region, double thickness = 2.0) {
  SheetShapeSpec spec;
  spec.thickness = thickness;
  spec.planar_patches.push_back({"F3", 0, Frame{}, named(std::move(region))});
  return spec;
}

ShapePtr only(const SheetShapeSpec& spec) {
  const SheetGeometry result = sheet_shape(spec);
  CHECK(result.diagnostics.empty());
  CHECK(result.solids.size() == 1);
  if (result.solids.size() != 1) {
    throw std::runtime_error("expected one sheet solid");
  }
  CHECK(result.solids.front().domains == std::vector<std::size_t>{0});
  return result.solids.front().shape;
}

void named_edges(const Shape& shape) {
  std::set<std::string> names;
  for (int i = 0; i < shape.face_count(); ++i) {
    CHECK(!shape.face_names(i).empty());
  }
  for (int i = 0; i < shape.edge_count(); ++i) {
    CHECK(!shape.edge_name(i).empty());
    CHECK(names.insert(shape.edge_name(i)).second);
    CHECK(shape.find_edges(shape.edge_name(i)) == std::vector<int>{i});
  }
}

double cap_z(const Shape& shape, const char* name) {
  const std::vector<int> faces = shape.find_faces(name);
  CHECK(faces.size() == 1);
  if (faces.size() != 1) {
    throw std::runtime_error("expected one named sheet surface");
  }
  const BoundingBox box = bounding_box(Shape(shape.face(faces.front())));
  CHECK(near(box.min.Z(), box.max.Z()));
  return box.min.Z();
}

void material_sides_and_parameters() {
  Region profile = rectangle_region(0, 0, 40, 20);
  profile.loops.push_back(circle_region(20, 10, 2).loops.front());
  SheetShapeSpec spec = one(profile);
  const ShapePtr front = only(spec);
  CHECK(near(volume(*front), 2 * (800 - 4 * kPi)));
  CHECK(front->face_count() == 7);
  CHECK(near(cap_z(*front, "F3:sheet_front(0)"), 0));
  CHECK(near(cap_z(*front, "F3:sheet_back(0)"), 2));
  CHECK(front->find_faces("F3:sheet_side0(F1:sheet_curve(c5))").size() == 1);
  named_edges(*front);

  spec.reference_side = SheetReferenceSide::Back;
  spec.thickness = 3;
  const ShapePtr back = only(spec);
  CHECK(near(volume(*back), 3 * (800 - 4 * kPi)));
  CHECK(near(cap_z(*back, "F3:sheet_front(0)"), -3));
  CHECK(near(cap_z(*back, "F3:sheet_back(0)"), 0));
  CHECK(near(volume(*front), 2 * (800 - 4 * kPi)));
  CHECK(near(spec.planar_patches.front().frame.origin.Z(), 0));
  CHECK(near(spec.planar_patches.front().region.loops[1].segments[0].radius, 2));
  named_edges(*back);

  // A rotated frame transports thickness along its normal, not world Z.
  spec.planar_patches.front().frame = {gp_Pnt(10, 20, 30), gp_Dir(0, 1, 0), gp_Dir(0, 0, 1)};
  const ShapePtr rotated = only(spec);
  const BoundingBox box = bounding_box(*rotated);
  CHECK(near(box.min.X(), 7) && near(box.max.X(), 10));
  CHECK(near(box.min.Y(), 20) && near(box.max.Y(), 60));
  CHECK(near(box.min.Z(), 30) && near(box.max.Z(), 50));
  CHECK(near(volume(*rotated), volume(*back)));
}

Segment line(double x0, double y0, double x1, double y1) {
  Segment result;
  result.kind = SegmentKind::Line;
  result.start = gp_Pnt2d(x0, y0);
  result.end = gp_Pnt2d(x1, y1);
  return result;
}

void curved_domains() {
  Region ring = circle_region(0, 0, 10);
  ring.loops.push_back(circle_region(0, 0, 4).loops.front());
  const ShapePtr annulus = only(one(ring));
  CHECK(near(volume(*annulus), 2 * kPi * (100 - 16)));
  CHECK(annulus->face_count() == 4);
  named_edges(*annulus);

  Segment arc;
  arc.kind = SegmentKind::Arc;
  arc.radius = 10;
  arc.start_angle = 0;
  arc.end_angle = kPi;
  const ShapePtr half_disc = only(one(Region{"", {Loop{{arc, line(-10, 0, 10, 0)}}}}));
  CHECK(near(volume(*half_disc), 100 * kPi));

  Segment ellipse;
  ellipse.kind = SegmentKind::Ellipse;
  ellipse.radius = 10;
  ellipse.minor_radius = 4;
  ellipse.rotation = kPi / 6;
  CHECK(near(volume(*only(one(Region{"", {Loop{{ellipse}}}}))), 80 * kPi));
  ellipse.kind = SegmentKind::EllipseArc;
  ellipse.rotation = 0;
  ellipse.start_angle = 0;
  ellipse.end_angle = kPi;
  CHECK(near(volume(*only(one(Region{"", {Loop{{ellipse, line(-10, 0, 10, 0)}}}}))), 40 * kPi));

  // Full circular and elliptical arcs keep their sweep across the periodic
  // seam, including a negative full turn and equal-angle representations.
  for (double span : {2 * kPi, -2 * kPi, 0.0}) {
    arc.start_angle = kPi / 3;
    arc.end_angle = arc.start_angle + span;
    CHECK(near(volume(*only(one(Region{"", {Loop{{arc}}}}))), 200 * kPi));
    ellipse.start_angle = kPi / 3;
    ellipse.end_angle = ellipse.start_angle + span;
    CHECK(near(volume(*only(one(Region{"", {Loop{{ellipse}}}}))), 80 * kPi));
  }

  // A rational quadratic curve remains a true quarter circle, with no
  // tessellated approximation of its support geometry or its material area.
  Segment spline;
  spline.kind = SegmentKind::BSpline;
  spline.degree = 2;
  spline.poles = {gp_Pnt2d(10, 0), gp_Pnt2d(10, 10), gp_Pnt2d(0, 10)};
  spline.weights = {1, std::sqrt(0.5), 1};
  spline.knots = {0, 1};
  spline.multiplicities = {3, 3};
  Region quarter{"", {Loop{{spline, line(0, 10, 0, 0), line(0, 0, 10, 0)}}}};
  CHECK(near(volume(*only(one(quarter))), 50 * kPi));

  // A single closed rational B-spline curve bounds a complete circle.
  spline.poles = {gp_Pnt2d(10, 0), gp_Pnt2d(10, 10), gp_Pnt2d(0, 10),
                  gp_Pnt2d(-10, 10), gp_Pnt2d(-10, 0), gp_Pnt2d(-10, -10),
                  gp_Pnt2d(0, -10), gp_Pnt2d(10, -10), gp_Pnt2d(10, 0)};
  spline.weights = {1, std::sqrt(0.5), 1, std::sqrt(0.5), 1,
                    std::sqrt(0.5), 1, std::sqrt(0.5), 1};
  spline.knots = {0, 1, 2, 3, 4};
  spline.multiplicities = {3, 2, 2, 2, 3};
  CHECK(near(volume(*only(one(Region{"", {Loop{{spline}}}}))), 200 * kPi));

  // Composite curves and glyph outlines can have several edges with one
  // stable source key. All side pieces must retain that lineage alias.
  SheetShapeSpec composite = one(rectangle_region(0, 0, 10, 5));
  for (Segment& segment : composite.planar_patches[0].region.loops[0].segments) {
    segment.name = "F1:sheet_curve(c1)";
  }
  const ShapePtr body = only(composite);
  CHECK(near(volume(*body), 100));
  CHECK(body->find_faces("F3:sheet_side0(F1:sheet_curve(c1))").size() == 4);
  named_edges(*body);
}

void domain_membership() {
  SheetShapeSpec spec = one(rectangle_region(30, 0, 10, 10));
  spec.planar_patches.push_back({"F3", 1, Frame{}, named(circle_region(0, 0, 3), 5)});
  SheetGeometry result = sheet_shape(spec);
  CHECK(result.solids.size() == 2);
  if (result.solids.size() == 2) {
    // The first input domain stays first even though the second is left of it.
    CHECK(result.solids[0].domains == std::vector<std::size_t>{0});
    CHECK(result.solids[1].domains == std::vector<std::size_t>{1});
    CHECK(near(volume(*result.solids[0].shape), 200));
    CHECK(near(volume(*result.solids[1].shape), 18 * kPi));
  }

  // A neighbouring panel joins at a whole side; both front/back aliases
  // survive the coplanar merge, and the internal wall disappears.
  spec.planar_patches[1].region = named(rectangle_region(40, 0, 10, 10), 5);
  result = sheet_shape(spec);
  CHECK(result.solids.size() == 1);
  if (result.solids.size() == 1) {
    CHECK((result.solids[0].domains == std::vector<std::size_t>{0, 1}));
    const Shape& body = *result.solids[0].shape;
    CHECK(near(volume(body), 400));
    CHECK(body.face_count() == 6);
    CHECK(body.find_faces("F3:sheet_front(0)") == body.find_faces("F3:sheet_front(1)"));
    CHECK(body.find_faces("F3:sheet_back(0)") == body.find_faces("F3:sheet_back(1)"));
    named_edges(body);
  }
}

void invalid_domains_and_tolerance() {
  const SheetShapeSpec valid = one(rectangle_region(0, 0, 10, 10));
  SheetShapeSpec spec = valid;
  spec.thickness = 0;
  CHECK(throws_with([&] { sheet_shape(spec); }, "sheet thickness"));
  spec.thickness = 1.0e-8;
  CHECK(throws_with([&] { sheet_shape(spec); }, "modelling tolerance"));
  spec = valid;
  spec.thickness = std::numeric_limits<double>::quiet_NaN();
  CHECK(throws_with([&] { sheet_shape(spec); }, "sheet thickness"));
  spec = valid;
  spec.planar_patches.push_back(spec.planar_patches.front());
  CHECK(throws_with([&] { sheet_shape(spec); }, "duplicate domain"));
  spec = valid;
  spec.planar_patches[0].frame.y_axis = gp_Dir(1, 1, 0);
  CHECK(throws_with([&] { sheet_shape(spec); }, "orthogonal"));
  spec = valid;
  spec.planar_patches.push_back({"F3", 1, Frame{}, named(circle_region(20, 0, 3), 5)});
  spec.planar_patches[1].frame.origin.SetZ(1);
  CHECK(throws_with([&] { sheet_shape(spec); }, "coplanar"));
  spec = valid;
  spec.planar_patches.push_back({"F3", 1, Frame{}, named(rectangle_region(5, 0, 10, 10), 5)});
  CHECK(throws_with([&] { sheet_shape(spec); }, "overlap"));
  spec.planar_patches[1].region = named(rectangle_region(10, 10, 10, 10), 5);
  CHECK(throws_with([&] { sheet_shape(spec); }, "zero-width"));
  spec = valid;
  spec.planar_patches[0].region.loops[0].segments.pop_back();
  CHECK(throws_with([&] { sheet_shape(spec); }, "not closed"));
  spec = one(Region{"", {Loop{{line(0, 0, 10, 10), line(10, 10, 0, 10),
                                line(0, 10, 10, 0), line(10, 0, 0, 0)}}}});
  CHECK(throws_with([&] { sheet_shape(spec); }, "invalid"));

  // A normal numerical join within the requested bound is accepted; the
  // imported-sketch default cannot silently close a gap beyond that bound.
  spec = valid;
  spec.planar_patches[0].region.loops[0].segments[1].start.SetY(-5.0e-8);
  CHECK(near(volume(*only(spec)), 200));
  spec.planar_patches[0].region.loops[0].segments[1].start.SetY(-1.1e-7);
  CHECK(throws_with([&] { sheet_shape(spec); }, "gap tolerance"));
  spec.planar_patches[0].region.loops[0].segments[1].start.SetY(-3.0e-6);
  CHECK(throws_with([&] { sheet_shape(spec); }, "gap tolerance"));
  spec.tolerance = 1.0e-5;
  CHECK(near(volume(*only(spec)), 200));

  spec = valid;
  spec.planar_patches[0].region.loops[0].segments[0].start.SetX(std::numeric_limits<double>::infinity());
  CHECK(throws_with([&] { sheet_shape(spec); }, "finite"));
}

class AfterChecks final : public CancelSource {
public:
  bool cancel_requested() const noexcept override { return ++m_checks >= 4; }
private:
  mutable int m_checks = 0;
};

void cancellation() {
  const SheetShapeSpec spec = one(rectangle_region(0, 0, 40, 20));
  CancelFlag flag;
  flag.cancel();
  {
    const CancelScope scope(flag);
    CHECK(throws_with([&] { sheet_shape(spec); }, "cancelled"));
  }
  {
    const AfterChecks source;
    const CancelScope scope(source);
    CHECK(throws_with([&] { sheet_shape(spec); }, "cancelled"));
  }
  CHECK(near(volume(*only(spec)), 1600));
}

} // namespace

MITCAD_BUNDLED_TEST(sheet) {
  enable_input_checks(true);
  test::guarded("sheet material sides", material_sides_and_parameters);
  test::guarded("sheet curved domains", curved_domains);
  test::guarded("sheet domain membership", domain_membership);
  test::guarded("sheet invalid domains", invalid_domains_and_tolerance);
  test::guarded("sheet cancellation", cancellation);
  for (const std::string& change : input_changes()) {
    std::fprintf(stderr, "%s\n", change.c_str());
  }
  CHECK(input_changes().empty());
  if (test::failures == 0) {
    std::puts("test_sheet: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
