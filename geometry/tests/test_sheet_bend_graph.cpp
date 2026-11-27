// SPDX-License-Identifier: MIT
// Independent real-geometry proof of opposite bends, partial opening and the
// exposed-clearance tear policy. Public timeline commands are tested separately.

#include <array>
#include <map>
#include <set>

#include <BRepAdaptor_Surface.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <TopExp.hxx>
#include <TopoDS_Face.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"
#include "mitcad/geometry/sheet_trim.hpp"
#include "bundled_test.hpp"

namespace {

using namespace mitcad::geometry;
constexpr double kWidth = 20;
constexpr double kR1 = 3;
constexpr double kR2 = 4;
constexpr double kMiddle = 15;
constexpr double kEnd = 12;
constexpr double kAngle = test::kPi / 2;

MaterialRegion rectangle(double u, double width, const std::string& feature) {
  MaterialLoop loop;
  std::uint32_t slot = 0;
  const auto region = rectangle_region(u, 0, width, kWidth);
  for (const auto& segment : region.loops.front().segments) {
    loop.edges.push_back({{feature + ":sheet_curve(" + std::to_string(slot++) + ")", 0},
                          {segment, {0, 1}}});
  }
  return {{loop}};
}

ShapePortSide side(const SheetPatch& patch, std::size_t index) {
  const auto& edge = patch.regions[0].loops[0].edges[index];
  const auto& line = edge.curve.geometry;
  return {{patch.domain, edge.piece}, edge.curve.range,
          {line.start.Y(), line.end.Y() - line.start.Y()}};
}

void transform(SheetPatch& patch, const gp_Trsf& transform) {
  if (auto* plane = std::get_if<PlaneEmbedding>(&patch.embedding)) {
    plane->front_frame.origin.Transform(transform);
    plane->front_frame.x_axis.Transform(transform);
    plane->front_frame.y_axis.Transform(transform);
    if (transform.IsNegative()) {
      plane->front_to_back_sign = plane->front_to_back_sign == MaterialParity::Positive
                                     ? MaterialParity::Negative : MaterialParity::Positive;
    }
  } else {
    auto& bend = std::get<CylinderEmbedding>(patch.embedding);
    bend.centre.Transform(transform);
    bend.axis.Transform(transform);
    bend.radial_start.Transform(transform);
    bend.angular_direction.Transform(transform);
  }
}

struct ZSheet {
  SheetChartShapeSpec shape;
  std::array<double, 5> offsets;
  gp_Trsf opening;
};

ZSheet z_sheet(double thickness, double k, bool opened = false) {
  ZSheet fixture;
  const double a1 = (kR1 + k * thickness) * kAngle;
  const double a2 = (kR2 + k * thickness) * kAngle;
  fixture.offsets = {0, 0, a1, a1 + kMiddle, a1 + kMiddle + a2};
  auto& spec = fixture.shape;
  spec.thickness = thickness;
  const PlaneEmbedding base{{{0, kWidth, -thickness}, {1, 0, 0}, {0, -1, 0}}, MaterialParity::Negative};
  const CylinderEmbedding first{{0, kWidth, kR1}, {0, -1, 0}, {0, 0, -1}, {1, 0, 0},
                                kR1, kR1 + k * thickness, kAngle, false};
  const gp_Pnt middle_origin(kR1 + thickness, kWidth, kR1);
  const PlaneEmbedding middle{{middle_origin, {0, 0, 1}, {0, -1, 0}}, MaterialParity::Negative};
  const gp_Pnt second_centre(kR1 + thickness + kR2, kWidth, kR1 + kMiddle);
  const CylinderEmbedding second{second_centre, {0, -1, 0}, {-1, 0, 0}, {0, 0, 1},
                                 kR2, kR2 + k * thickness, kAngle, true};
  const PlaneEmbedding end{{{second_centre.X(), kWidth, second_centre.Z() + kR2}, {1, 0, 0}, {0, -1, 0}},
                            MaterialParity::Negative};
  spec.patches = {
      {{SheetDomainKind::Panel, "F10", 0}, {rectangle(-30, 30, "F20")}, base},
      {{SheetDomainKind::Bend, "F11", 0}, {rectangle(0, a1, "F21")}, first},
      {{SheetDomainKind::Panel, "F11", 0}, {rectangle(0, kMiddle, "F22")}, middle},
      {{SheetDomainKind::Bend, "F12", 0}, {rectangle(0, a2, "F23")}, second},
      {{SheetDomainKind::Panel, "F12", 0}, {rectangle(0, kEnd, "F24")}, end}};
  for (std::size_t i = 1; i < spec.patches.size(); ++i) {
    spec.ports.push_back({static_cast<std::uint32_t>(i), {0, kWidth},
                          side(spec.patches[i - 1], 1), side(spec.patches[i], 3)});
  }
  fixture.opening.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0)), kAngle);
  const gp_Pnt destination(a1, kWidth, -thickness);
  fixture.opening.SetTranslationPart(gp_Vec(middle_origin.Transformed(fixture.opening), destination));
  if (opened) {
    spec.patches[1].embedding = base;
    for (std::size_t i = 2; i < spec.patches.size(); ++i) {
      transform(spec.patches[i], fixture.opening);
    }
  }
  return fixture;
}

std::string key(const SheetDomainRef& domain) {
  return domain.feature + (domain.kind == SheetDomainKind::Panel ? ".p" : ".b") +
         std::to_string(domain.slot);
}

void cut(ZSheet& fixture, const MaterialRegion& tool) {
  SheetTrimSpec spec;
  for (std::size_t i = 0; i < fixture.shape.patches.size(); ++i) {
    const auto& patch = fixture.shape.patches[i];
    spec.domains.push_back({patch.domain, patch.regions, {{fixture.offsets[i], 0}, {1, 0}, {0, 1}}});
  }
  for (const auto& port : fixture.shape.ports) {
    spec.seams.push_back({port.token, {port.first.boundary, port.first.span, port.first.axial},
                          {port.second.boundary, port.second.span, port.second.axial}, port.axial_interval});
  }
  spec.tools = {tool};
  const auto trimmed = sheet_trim_domain(spec);
  CHECK(trimmed.domains.size() == 5);
  std::map<std::pair<std::string, std::uint32_t>, MaterialEdge> edges;
  for (auto& patch : fixture.shape.patches) {
    patch.regions.clear();
    for (const auto& domain : trimmed.domains) {
      if (domain.domain != patch.domain) {
        continue;
      }
      for (const auto& region : domain.regions) {
        MaterialRegion material;
        for (const auto& loop : region.loops) {
          MaterialLoop boundary;
          for (const auto& edge : loop.edges) {
            CHECK(!edge.lineage.empty());
            MaterialEdge settled{{"F90:sheet_curve(" + std::to_string(edge.token) + ")", 0}, edge.curve};
            edges.emplace(std::make_pair(key(domain.domain), edge.token), settled);
            boundary.edges.push_back(std::move(settled));
          }
          material.loops.push_back(std::move(boundary));
        }
        patch.regions.push_back(std::move(material));
      }
    }
  }
  fixture.shape.ports.clear();
  for (const auto& port : trimmed.ports) {
    auto mapped = [&](const TrimEndpointSpan& endpoint) {
      const auto& edge = edges.at({key(endpoint.domain), endpoint.edge_token});
      CHECK(edge.curve.geometry.kind == SegmentKind::Line);
      const auto& line = edge.curve.geometry;
      return ShapePortSide{{endpoint.domain, edge.piece}, endpoint.range,
                            {line.start.Y(), line.end.Y() - line.start.Y()}};
    };
    fixture.shape.ports.push_back({static_cast<std::uint32_t>(fixture.shape.ports.size()),
                                   port.axial_interval, mapped(port.first), mapped(port.second)});
  }
}

MaterialRegion circular_mask(double allowance) {
  Segment circle;
  circle.kind = SegmentKind::Circle;
  circle.center = {allowance / 2, kWidth / 2};
  circle.radius = 4;
  MaterialLoop loop;
  loop.edges.push_back({{"F80:sheet_curve(0)", 0}, {circle, {0, 2 * test::kPi}}});
  return {{loop}};
}

ShapePtr only(const ZSheet& fixture) {
  const auto built = sheet_shape(fixture.shape);
  CHECK(built.solids.size() == 1);
  CHECK(built.diagnostics.empty());
  if (built.solids.size() != 1) {
    throw std::runtime_error("expected one connected Z sheet solid");
  }
  const auto shape = built.solids[0].shape;
  CHECK(BRepCheck_Analyzer(shape->occt()).IsValid());
  std::set<std::string> domains;
  for (const auto& membership : built.solids[0].membership) {
    domains.insert(key(membership.domain));
  }
  CHECK(domains.size() == 5);
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> ancestors;
  TopExp::MapShapesAndAncestors(shape->occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  for (int i = 1; i <= ancestors.Extent(); ++i) {
    CHECK(ancestors(i).Size() == 2);
  }
  for (int f = 0; f < shape->face_count(); ++f) {
    CHECK(!shape->face_names(f).empty());
  }
  return shape;
}

double folded_volume(double t) {
  return kWidth * (t * (30 + kMiddle + kEnd) + kAngle * (t * (kR1 + kR2) + t * t));
}

double partial_volume(double t, double k) {
  return folded_volume(t) + kWidth * kAngle * t * t * (k - .5);
}

void same_second_bend(const ZSheet& original, const ZSheet& opened, const Shape& actual) {
  const auto& before = std::get<CylinderEmbedding>(original.shape.patches[3].embedding);
  const auto& after = std::get<CylinderEmbedding>(opened.shape.patches[3].embedding);
  CHECK(before.inner_is_front && after.inner_is_front);
  CHECK_NEAR(after.inside_radius, before.inside_radius);
  CHECK_NEAR(after.neutral_radius, before.neutral_radius);
  CHECK_NEAR(after.angle_extent, before.angle_extent);
  CHECK(after.centre.Distance(before.centre.Transformed(opened.opening)) < 1.0e-10);
  CHECK(after.radial_start.IsEqual(before.radial_start.Transformed(opened.opening), 1.0e-10));
  CHECK(after.angular_direction.IsEqual(before.angular_direction.Transformed(opened.opening), 1.0e-10));
  int cylinders = 0;
  for (int f = 0; f < actual.face_count(); ++f) {
    BRepAdaptor_Surface surface(actual.face(f));
    if (surface.GetType() != GeomAbs_Cylinder) {
      continue;
    }
    const auto cylinder = surface.Cylinder();
    const double radius = cylinder.Radius();
    CHECK(test::near(radius, kR2) || test::near(radius, kR2 + opened.shape.thickness));
    CHECK(gp_Vec(after.centre, cylinder.Location()).Crossed(gp_Vec(after.axis)).Magnitude() < 1.0e-7);
    ++cylinders;
  }
  CHECK(cylinders == 2); // B1 is planar; only the carried B2 remains curved.
}

void partial_open_cut_refold() {
  for (const auto& parameters : std::array<std::array<double, 2>, 2>{{{{2, .3}}, {{3, .6}}}}) {
    const double t = parameters[0], k = parameters[1];
    const ZSheet folded = z_sheet(t, k);
    ZSheet opened = z_sheet(t, k, true);
    const auto uncut_folded = only(folded);
    const auto uncut_opened = only(opened);
    CHECK_NEAR_TOL(volume(*uncut_folded), folded_volume(t), 1.0e-8);
    CHECK_NEAR_TOL(volume(*uncut_opened), partial_volume(t, k), 1.0e-8);
    same_second_bend(folded, opened, *uncut_opened);
    const auto& original_base = std::get<PlaneEmbedding>(folded.shape.patches[0].embedding);
    const auto& fixed_base = std::get<PlaneEmbedding>(opened.shape.patches[0].embedding);
    CHECK(original_base.front_frame.origin.Distance(fixed_base.front_frame.origin) < 1.0e-12);
    CHECK(original_base.front_frame.x_axis.IsEqual(fixed_base.front_frame.x_axis, 1.0e-12));

    const double allowance = (kR1 + k * t) * kAngle;
    cut(opened, circular_mask(allowance));
    const auto cut_open = only(opened);
    CHECK_NEAR_TOL(volume(*cut_open), partial_volume(t, k) - t * 16 * test::kPi, 1.0e-8);
    same_second_bend(folded, opened, *cut_open);
    // Refold from the changed chart domains, not from a saved pre-cut B-rep.
    ZSheet refolded = opened;
    for (std::size_t i = 0; i < refolded.shape.patches.size(); ++i) {
      refolded.shape.patches[i].embedding = folded.shape.patches[i].embedding;
    }
    const auto cut_fold = only(refolded);
    const double half = allowance / 2;
    const double bend_mask_area = allowance * std::sqrt(16 - half * half) + 32 * std::asin(half / 4);
    const double removed = t * (16 * test::kPi - bend_mask_area) +
                           t * (kR1 + t / 2) / (kR1 + k * t) * bend_mask_area;
    CHECK_NEAR_TOL(volume(*cut_fold), folded_volume(t) - removed, 1.0e-8);
    for (std::size_t i = 0; i < refolded.shape.patches.size(); ++i) {
      refolded.shape.patches[i].embedding = opened.shape.patches[i].embedding;
    }
    const auto reopened = only(refolded);
    CHECK_NEAR_TOL(volume(*reopened), volume(*cut_open), 1.0e-10);
    const auto before = bounding_box(*cut_open), after = bounding_box(*reopened);
    CHECK(before.min.Distance(after.min) < 1.0e-7 && before.max.Distance(after.max) < 1.0e-7);
  }
}

void reversed_sign_and_stationary_branch() {
  const ZSheet original = z_sheet(2, .3);
  ZSheet reversed = original;
  gp_Trsf mirror;
  mirror.SetMirror(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)));
  for (auto& patch : reversed.shape.patches) {
    transform(patch, mirror);
  }
  const auto normal_shape = only(original), reflected = only(reversed);
  CHECK_NEAR_TOL(volume(*reflected), volume(*normal_shape), 1.0e-10);
  const auto a = bounding_box(*normal_shape), b = bounding_box(*reflected);
  CHECK_NEAR_TOL(a.min.Z(), -b.max.Z(), 1.0e-8);
  CHECK_NEAR_TOL(a.max.Z(), -b.min.Z(), 1.0e-8);

  // The other stationary-side choice is the rigid inverse of the opening
  // placement: B2 and its branch stay fixed while the former base moves.
  ZSheet other_side = z_sheet(2, .3, true);
  for (auto& patch : other_side.shape.patches) {
    transform(patch, other_side.opening.Inverted());
  }
  const auto& stationary = std::get<CylinderEmbedding>(other_side.shape.patches[3].embedding);
  const auto& prior = std::get<CylinderEmbedding>(original.shape.patches[3].embedding);
  CHECK(stationary.centre.Distance(prior.centre) < 1.0e-10);
  CHECK(stationary.radial_start.IsEqual(prior.radial_start, 1.0e-10));
  CHECK(stationary.angular_direction.IsEqual(prior.angular_direction, 1.0e-10));
  CHECK_NEAR_TOL(volume(*only(other_side)), partial_volume(2, .3), 1.0e-8);
}

MaterialRegion tear_mask(double gap, double depth, double end) {
  // Explicit square-ended slit clearance; no tolerance-sized hidden gap.
  constexpr double offset = 3;
  const std::array<gp_Pnt2d, 6> points{{{0, 0}, {end, 0}, {end, offset},
                                      {-depth, offset}, {-depth, offset - gap}, {0, offset - gap}}};
  MaterialLoop loop;
  for (std::size_t i = 0; i < points.size(); ++i) {
    Segment line;
    line.kind = SegmentKind::Line;
    line.start = points[i];
    line.end = points[(i + 1) % points.size()];
    loop.edges.push_back({{"F95:sheet_curve(" + std::to_string(i) + ")", 0}, {line, {0, 1}}});
  }
  return {{loop}};
}

void exposed_tear_clearance() {
  constexpr double t = 2, depth = 2, offset = 3;
  double previous_volume = 0;
  for (double gap : {.4, .8}) {
    ZSheet fixture = z_sheet(t, .3);
    const double end = fixture.offsets.back() + kEnd + 1;
    // The relief explicitly detaches the end tab. Its former seam interval is
    // no longer a join; keep only the width that still joins the moving branch.
    auto& detached = fixture.shape.ports.front();
    detached.axial_interval.first = offset;
    for (auto* port_side : {&detached.first, &detached.second}) {
      port_side->span = {(offset - port_side->axial.offset) / port_side->axial.scale,
                         (kWidth - port_side->axial.offset) / port_side->axial.scale};
    }
    cut(fixture, tear_mask(gap, depth, end));
    const auto shape = only(fixture);
    const double removed_branch = offset * (t * (kMiddle + kEnd) + kAngle * (t * (kR1 + kR2) + t * t));
    const double expected = folded_volume(t) - removed_branch - t * depth * gap;
    CHECK_NEAR_TOL(volume(*shape), expected, 1.0e-8);
    BRepClass3d_SolidClassifier gap_point(shape->occt(), {-depth / 2, kWidth - offset + gap / 2, -t / 2}, 1.0e-7);
    CHECK(gap_point.State() == TopAbs_OUT);
    BRepClass3d_SolidClassifier tab_point(shape->occt(), {-depth / 2, kWidth - offset + gap + .2, -t / 2}, 1.0e-7);
    CHECK(tab_point.State() == TopAbs_IN);
    BRepClass3d_SolidClassifier behind_tip(shape->occt(), {-depth - .2, kWidth - offset + gap / 2, -t / 2}, 1.0e-7);
    CHECK(behind_tip.State() == TopAbs_IN);
    if (previous_volume > 0) {
      CHECK_NEAR_TOL(previous_volume - volume(*shape), t * depth * .4, 1.0e-8);
    }
    previous_volume = volume(*shape);
  }
}

} // namespace

MITCAD_BUNDLED_TEST(sheet_bend_graph) {
  enable_input_checks(true);
  test::guarded("sheet Z partial opening and changed material mask", partial_open_cut_refold);
  test::guarded("sheet reverse signs and stationary branch", reversed_sign_and_stationary_branch);
  test::guarded("sheet exposed tear clearance topology", exposed_tear_clearance);
  CHECK(input_changes().empty());
  if (test::failures == 0) {
    std::puts("test_sheet_bend_graph: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
