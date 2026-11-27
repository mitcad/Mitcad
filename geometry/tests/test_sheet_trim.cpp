// SPDX-License-Identifier: MIT
// Real planar Booleans and exact curve transport for the sheet chart seam.

#include <algorithm>
#include <limits>
#include <set>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI.hxx>
#include <Geom2dAPI_ProjectPointOnCurve.hxx>
#include <ShapeFix_Face.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pln.hxx>

#include "check.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/sheet_trim.hpp"
#include "../src/sheet_curve.hpp"
#include "bundled_test.hpp"

namespace {

using namespace mitcad::geometry;
using test::near;
using test::throws_with;
constexpr double kPi = test::kPi;
const char* area_context = "unspecified";

MaterialRegion named(Region input, const std::string& prefix) {
  MaterialRegion result;
  std::uint32_t index = 0;
  for (const auto& loop : input.loops) {
    MaterialLoop output;
    for (const auto& segment : loop.segments) {
      ParamSpan range{0, 1};
      if (segment.kind == SegmentKind::Circle || segment.kind == SegmentKind::Ellipse) {
        range = {0, 2 * kPi};
      } else if (segment.kind == SegmentKind::Arc || segment.kind == SegmentKind::EllipseArc) {
        range = {segment.start_angle, segment.end_angle};
      } else if (segment.kind == SegmentKind::BSpline) {
        range = {segment.knots.front(), segment.knots.back()};
      }
      output.edges.push_back({{prefix + std::to_string(index++), 0}, {segment, range}});
    }
    result.loops.push_back(std::move(output));
  }
  return result;
}

TrimDomain rectangle(std::uint32_t slot, double x, double y, double width, double height) {
  return {{SheetDomainKind::Panel, "F2", slot},
          {named(rectangle_region(x, y, width, height), "edge" + std::to_string(slot) + "/")}, {}};
}

double area(const TrimRegion& region) {
  MaterialRegion material;
  for (const auto& loop : region.loops) {
    MaterialLoop boundary;
    for (const auto& edge : loop.edges) {
      boundary.edges.push_back({{"test" + std::to_string(edge.token), 0}, edge.curve});
    }
    material.loops.push_back(std::move(boundary));
  }
  const auto built = detail::make_sheet_face(material, {}, 1.0e-7);
  const bool valid = BRepCheck_Analyzer(built.face).IsValid();
  if (!valid) {
    std::fprintf(stderr, "invalid reconstructed planar face: %s, region %u, loops %zu\n",
                 area_context, region.token, region.loops.size());
    for (const auto& loop : region.loops) {
      for (const auto& edge : loop.edges) {
        std::fprintf(stderr, "  edge %u kind %d range %.17g %.17g\n", edge.token,
                     static_cast<int>(edge.curve.geometry.kind), edge.curve.range.first, edge.curve.range.last);
      }
    }
  }
  CHECK(valid);
  GProp_GProps properties;
  // Fixed quadrature is insufficient for trimmed rational B-spline boundaries.
  const double error = BRepGProp::SurfaceProperties(built.face, properties, 1.0e-12);
  CHECK(error <= 1.0e-12);
  return properties.Mass();
}

double total_area(const SheetTrimResult& result) {
  double total = 0;
  for (const auto& domain : result.domains) {
    for (const auto& region : domain.regions) {
      total += area(region);
    }
  }
  return total;
}

void check_lineage(const SheetTrimResult& result) {
  std::set<std::uint32_t> tokens;
  for (const auto& domain : result.domains) {
    for (const auto& region : domain.regions) {
      for (const auto& loop : region.loops) {
        CHECK(!loop.edges.empty());
        for (std::size_t i = 0; i < loop.edges.size(); ++i) {
          const auto& edge = loop.edges[i];
          CHECK(tokens.insert(edge.token).second);
          CHECK(!edge.lineage.empty());
          for (const auto& source : edge.lineage) {
            CHECK(!source.source.piece.curve.empty());
            CHECK(std::isfinite(source.span.first) && std::isfinite(source.span.last));
            CHECK(source.span.first != source.span.last);
          }
          const auto a = detail::make_sheet_curve(edge.curve);
          const auto b = detail::make_sheet_curve(loop.edges[(i + 1) % loop.edges.size()].curve);
          CHECK(a.curve->Value(a.native_range.last).Distance(b.curve->Value(b.native_range.first)) < 1.0e-7);
        }
      }
    }
  }
  for (const auto& port : result.ports) {
    CHECK(tokens.count(port.first.edge_token) == 1);
    CHECK(tokens.count(port.second.edge_token) == 1);
  }
  for (const auto& split : result.splits) {
    for (const auto& endpoint : split.first) {
      CHECK(tokens.count(endpoint.edge_token) == 1);
    }
    for (const auto& endpoint : split.second) {
      CHECK(tokens.count(endpoint.edge_token) == 1);
    }
  }
}

TrimSeam seam(const TrimDomain& left, const TrimDomain& right, std::uint32_t token) {
  return {token,
      {{left.domain, left.regions[0].loops[0].edges[1].piece}, {0, 1}, {0, 20}},
      {{right.domain, right.regions[0].loops[0].edges[3].piece}, {0, 1}, {20, -20}},
      {0, 20}};
}

SheetTrimSpec l_sheet() {
  const double allowance = 1.8 * kPi;
  SheetTrimSpec spec;
  spec.domains = {rectangle(0, -30, 0, 30, 20), rectangle(1, 0, 0, allowance, 20),
                  rectangle(2, allowance, 0, 15, 20)};
  spec.domains[1].domain.kind = SheetDomainKind::Bend;
  spec.seams = {seam(spec.domains[0], spec.domains[1], 1), seam(spec.domains[1], spec.domains[2], 2)};
  return spec;
}

void circle_across_two_seams() {
  area_context = "circle across two seams";
  SheetTrimSpec spec = l_sheet();
  const double allowance = 1.8 * kPi;
  spec.tools = {named(circle_region(allowance / 2, 10, 4), "circle")};
  const auto result = sheet_trim_domain(spec);
  CHECK(result.domains.size() == 3);
  CHECK_NEAR_TOL(total_area(result), 20 * (45 + allowance) - 16 * kPi, 1.0e-9);
  check_lineage(result);
  const double height = std::sqrt(16 - allowance * allowance / 4);
  for (std::uint32_t token : {1U, 2U}) {
    std::vector<double> crossings;
    for (const auto& split : result.splits) {
      if (split.seam_token == token && split.kind == SeamEventKind::Crossing) {
        CHECK(!split.first.empty() && !split.second.empty());
        crossings.push_back(split.axial);
      }
    }
    CHECK(crossings.size() == 2);
    if (crossings.size() == 2) {
      CHECK_NEAR_TOL(crossings[0], 10 - height, 1.0e-9);
      CHECK_NEAR_TOL(crossings[1], 10 + height, 1.0e-9);
    }
    std::vector<ParamSpan> retained;
    for (const auto& port : result.ports) {
      if (port.seam_token == token) {
        retained.push_back(port.axial_interval);
      }
    }
    CHECK(retained.size() == 2);
    if (retained.size() == 2) {
      CHECK_NEAR_TOL(retained[0].first, 0, 1.0e-9);
      CHECK_NEAR_TOL(retained[0].last, 10 - height, 1.0e-9);
      CHECK_NEAR_TOL(retained[1].first, 10 + height, 1.0e-9);
      CHECK_NEAR_TOL(retained[1].last, 20, 1.0e-9);
    }
  }
  int arcs = 0;
  for (const auto& domain : result.domains) {
    for (const auto& region : domain.regions) {
      for (const auto& loop : region.loops) {
        for (const auto& edge : loop.edges) {
          const bool from_circle = std::any_of(edge.lineage.begin(), edge.lineage.end(), [](const SourceSpan& source) {
            return source.source.kind == TrimSourceKind::Tool && source.source.piece.curve == "circle0";
          });
          if (from_circle) {
            ++arcs;
            CHECK(edge.curve.geometry.kind == SegmentKind::Arc);
            CHECK_NEAR(edge.curve.geometry.radius, 4);
            CHECK(edge.curve.geometry.center.Distance(gp_Pnt2d(allowance / 2, 10)) < 1.0e-9);
          }
        }
      }
    }
  }
  CHECK(arcs >= 4);
  // Source supports remain untouched, and a repeat recompute has equal material.
  CHECK(spec.tools[0].loops[0].edges[0].curve.geometry.kind == SegmentKind::Circle);
  CHECK_NEAR_TOL(total_area(sheet_trim_domain(spec)), total_area(result), 1.0e-10);

  spec.tools = {named(rectangle_region(-2, 8, allowance + 4, 4), "rectangle")};
  area_context = "rectangle across two seams";
  const auto rectangular = sheet_trim_domain(spec);
  CHECK_NEAR_TOL(total_area(rectangular), 20 * (45 + allowance) - 4 * (allowance + 4), 1.0e-10);
  CHECK(rectangular.ports.size() == 4);
  check_lineage(rectangular);

  // A circular hole tangent to an internal semantic seam introduces a shared
  // split but removes no interval of that seam. Both material charts survive.
  spec.tools = {named(circle_region(-4, 10, 4), "tangent")};
  area_context = "circle tangent to internal seam";
  const auto tangent = sheet_trim_domain(spec);
  CHECK_NEAR_TOL(total_area(tangent), 20 * (45 + allowance) - 16 * kPi, 1.0e-9);
  CHECK(std::any_of(tangent.splits.begin(), tangent.splits.end(), [](const SeamSplit& split) {
    return split.seam_token == 1 && split.kind == SeamEventKind::Tangency &&
           near(split.axial, 10) && !split.first.empty() && !split.second.empty();
  }));
  check_lineage(tangent);

  // A cut ending exactly on a tangent line removes material on one side only
  // (mitcad#209): that interval stops being a seam and the other side keeps
  // it as a free boundary.
  spec.tools = {named(rectangle_region(-6, 2, 6, 4), "flush")};
  area_context = "rectangle ending on a seam";
  const auto flush = sheet_trim_domain(spec);
  CHECK_NEAR_TOL(total_area(flush), 20 * (45 + allowance) - 24, 1.0e-10);
  check_lineage(flush);
  std::vector<ParamSpan> kept;
  for (const auto& port : flush.ports) {
    if (port.seam_token == 1) {
      kept.push_back(port.axial_interval);
    }
  }
  CHECK(kept.size() == 2);
  if (kept.size() == 2) {
    CHECK_NEAR_TOL(kept[0].first, 0, 1.0e-9);
    CHECK_NEAR_TOL(kept[0].last, 2, 1.0e-9);
    CHECK_NEAR_TOL(kept[1].first, 6, 1.0e-9);
    CHECK_NEAR_TOL(kept[1].last, 20, 1.0e-9);
  }
}

void holes_islands_and_ownership() {
  area_context = "holes islands ownership";
  SheetTrimSpec spec;
  spec.domains = {rectangle(0, 0, 0, 20, 10)};
  spec.tools = {named(circle_region(5, 5, 2), "hole")};
  const auto hole = sheet_trim_domain(spec);
  CHECK(hole.domains[0].regions.size() == 1);
  CHECK(hole.domains[0].regions[0].loops.size() == 2);
  CHECK_NEAR_TOL(total_area(hole), 200 - 4 * kPi, 1.0e-10);
  check_lineage(hole);
  spec.tools = {named(rectangle_region(9, -1, 2, 12), "split")};
  const auto split = sheet_trim_domain(spec);
  CHECK(split.domains[0].regions.size() == 2);
  CHECK_NEAR_TOL(total_area(split), 180, 1.0e-10);
  check_lineage(split);
  spec.operation = SheetTrimOperation::Intersection;
  CHECK_NEAR_TOL(total_area(sheet_trim_domain(spec)), 20, 1.0e-10);
  spec.tools.clear();
  CHECK(sheet_trim_domain(spec).domains[0].regions.empty());
  spec.operation = SheetTrimOperation::Union;
  spec.tools = {named(rectangle_region(30, 0, 5, 5), "island")};
  const auto united = sheet_trim_domain(spec);
  CHECK(united.domains[0].regions.size() == 2);
  CHECK_NEAR_TOL(total_area(united), 225, 1.0e-10);
  spec.domains.push_back(rectangle(1, 40, 0, 3, 3));
  CHECK(throws_with([&] { sheet_trim_domain(spec); }, "exactly one material domain"));

  // Coincident geometry has two proven ancestors. An alias must not disappear
  // because another source happened to be encountered first in the Boolean.
  spec.domains = {rectangle(0, 0, 0, 20, 10)};
  spec.tools = {named(rectangle_region(0, 0, 20, 10), "alias")};
  const auto aliases = sheet_trim_domain(spec);
  CHECK_NEAR_TOL(total_area(aliases), 200, 1.0e-10);
  for (const auto& edge : aliases.domains[0].regions[0].loops[0].edges) {
    CHECK(std::any_of(edge.lineage.begin(), edge.lineage.end(), [](const SourceSpan& source) {
      return source.source.kind == TrimSourceKind::Domain;
    }));
    CHECK(std::any_of(edge.lineage.begin(), edge.lineage.end(), [](const SourceSpan& source) {
      return source.source.kind == TrimSourceKind::Tool;
    }));
  }
}

void exact_supports_and_reflection() {
  area_context = "exact rational spline and reflection";
  // A rational quadratic describes the quarter unit circle exactly. Preserve
  // its native [2,5] parameter, weights and bounded reversed use through trim.
  Segment spline;
  spline.kind = SegmentKind::BSpline;
  spline.degree = 2;
  spline.poles = {{1, 0}, {1, 1}, {0, 1}};
  spline.weights = {1, std::sqrt(0.5), 1};
  spline.knots = {2, 5};
  spline.multiplicities = {3, 3};
  BoundedCurve2 bounded{spline, {2, 5}};
  const auto native = detail::make_sheet_curve(bounded);
  const auto exported = detail::export_sheet_curve(native.curve, native.native_range);
  CHECK(exported.geometry.kind == SegmentKind::BSpline);
  CHECK(exported.geometry.weights == spline.weights);
  CHECK(exported.geometry.knots == spline.knots);
  for (int i = 0; i <= 20; ++i) {
    const auto p = native.curve->Value(2 + 3.0 * i / 20);
    CHECK_NEAR_TOL(p.X() * p.X() + p.Y() * p.Y(), 1, 1.0e-12);
  }
  MaterialRegion quarter;
  auto line = [](gp_Pnt2d a, gp_Pnt2d b) {
    Segment result;
    result.kind = SegmentKind::Line;
    result.start = a;
    result.end = b;
    return BoundedCurve2{result, {0, 1}};
  };
  quarter.loops = {{{{{"radial0", 0}, line({0, 0}, {1, 0})},
                      {{"rational", 0}, bounded},
                      {{"radial1", 0}, line({0, 1}, {0, 0})}}}};
  SheetTrimSpec spec;
  spec.domains = {{{SheetDomainKind::Panel, "F5", 0}, {quarter}, {}}};
  spec.tools = {named(rectangle_region(-2, -2, 2.5, 4), "clip")};
  const auto cut = sheet_trim_domain(spec);
  // Unit quarter disk right of x=.5: integral_0.5^1 sqrt(1-x*x) dx.
  CHECK_NEAR_TOL(total_area(cut), kPi / 6 - std::sqrt(3.0) / 8, 1.0e-9);
  bool rational_found = false;
  for (const auto& region : cut.domains[0].regions) {
    for (const auto& edge : region.loops[0].edges) {
      if (edge.curve.geometry.kind == SegmentKind::BSpline) {
        rational_found = true;
        CHECK(!edge.curve.geometry.weights.empty());
        CHECK(std::any_of(edge.lineage.begin(), edge.lineage.end(), [](const SourceSpan& source) {
          return source.source.piece.curve == "rational" &&
                 std::min(source.span.first, source.span.last) >= 2 &&
                 std::max(source.span.first, source.span.last) <= 5;
        }));
      }
    }
  }
  CHECK(rational_found);
  check_lineage(cut);

  Segment ellipse;
  ellipse.kind = SegmentKind::Ellipse;
  ellipse.center = {0, 0};
  ellipse.radius = 4;
  ellipse.minor_radius = 2;
  ellipse.rotation = 0.4;
  MaterialRegion oval{{{{{{"ellipse", 0}, {ellipse, {0, 2 * kPi}}}}}}};
  spec.domains = {{{SheetDomainKind::Panel, "F6", 0}, {oval},
                   {{10, 20}, {0, 1}, {1, 0}}}}; // reflected operation chart
  spec.tools.clear();
  const auto unchanged = sheet_trim_domain(spec);
  CHECK_NEAR_TOL(total_area(unchanged), 8 * kPi, 1.0e-10);
  const auto& actual = unchanged.domains[0].regions[0].loops[0].edges[0].curve;
  CHECK(actual.geometry.kind == SegmentKind::Ellipse);
  CHECK(actual.geometry.center.Distance({0, 0}) < 1.0e-10);
  CHECK_NEAR_TOL(actual.geometry.rotation, .4, 1.0e-10);
  check_lineage(unchanged);

  // Exact reverse-conversion also handles a full-period reversed circle and
  // native spline parameters under orientation-reversing isometries.
  Segment circle;
  circle.kind = SegmentKind::Circle;
  circle.center = {3, -2};
  circle.radius = 2;
  const Isometry2 mirror{{10, 20}, {0, 1}, {1, 0}};
  for (const auto& input : std::vector<BoundedCurve2>{{circle, {2 * kPi, 0}},
                                                     {ellipse, {2 * kPi, 0}},
                                                     {spline, {5, 2}}}) {
    const auto original = detail::make_sheet_curve(input);
    const auto placed = detail::transform_sheet_curve(original.curve, mirror);
    const auto converted = detail::export_sheet_curve(placed, original.native_range);
    const auto rebuilt = detail::make_sheet_curve(converted);
    for (int i = 0; i <= 20; ++i) {
      const double fraction = i / 20.0;
      const auto expected = placed->Value(original.native_range.first +
          fraction * (original.native_range.last - original.native_range.first));
      const auto observed = rebuilt.curve->Value(rebuilt.native_range.first +
          fraction * (rebuilt.native_range.last - rebuilt.native_range.first));
      CHECK(expected.Distance(observed) < 1.0e-10);
    }
  }
}

void periodic_spline_transport() {
  area_context = "bounded periodic rational spline";
  Segment spline;
  spline.kind = SegmentKind::BSpline;
  spline.degree = 2;
  spline.periodic = true;
  spline.poles = {{1, 0}, {0, 1}, {-1, 0}, {0, -1}};
  spline.weights = {1, 2, 1, 2};
  spline.knots = {0, 1, 2, 3, 4};
  spline.multiplicities = {1, 1, 1, 1, 1};
  const auto support = detail::make_sheet_curve({spline, {0, 4}});
  const Isometry2 reflection{{10, 20}, {0, 1}, {1, 0}};
  for (const ParamSpan range : std::vector<ParamSpan>{{3.5, 4.5}, {4.5, 3.5}, {4, 0}}) {
    const auto transformed = detail::transform_sheet_curve(support.curve, reflection);
    const auto exported = detail::export_sheet_curve(transformed, range);
    CHECK(exported.geometry.kind == SegmentKind::BSpline);
    CHECK(!exported.geometry.periodic);
    CHECK(!exported.geometry.weights.empty());
    CHECK_NEAR_TOL(exported.range.first, range.first, 1.0e-12);
    CHECK_NEAR_TOL(exported.range.last, range.last, 1.0e-12);
    const auto rebuilt = detail::make_sheet_curve(exported);
    for (int i = 0; i <= 40; ++i) {
      const double parameter = range.first + (range.last - range.first) * i / 40.0;
      CHECK(transformed->Value(parameter).Distance(rebuilt.curve->Value(parameter)) < 1.0e-10);
    }
    CHECK(transformed->IsPeriodic()); // Export must not mutate shared support.
  }
  SheetTrimSpec spec;
  spec.domains = {{{SheetDomainKind::Panel, "F8", 0},
                   {named(Region{{}, {{{spline}}}}, "periodic")}, {}}};
  const auto full = sheet_trim_domain(spec);
  CHECK(full.domains[0].regions.size() == 1);
  check_lineage(full);
  spec.tools = {named(rectangle_region(0, -2, 2, 4), "half")};
  const auto half = sheet_trim_domain(spec);
  CHECK(half.domains[0].regions.size() == 1);
  // Opposite poles and weights give central symmetry; x = 0 bisects its area.
  CHECK_NEAR_TOL(total_area(half), total_area(full) / 2, 1.0e-9);
  check_lineage(half);
  int bounded_splines = 0;
  for (const auto& region : half.domains[0].regions) {
    for (const auto& loop : region.loops) {
      for (const auto& edge : loop.edges) {
        if (edge.curve.geometry.kind != SegmentKind::BSpline) {
          continue;
        }
        ++bounded_splines;
        CHECK(!edge.curve.geometry.periodic);
        CHECK(!edge.curve.geometry.weights.empty());
        CHECK(std::any_of(edge.lineage.begin(), edge.lineage.end(), [](const SourceSpan& source) {
          return source.source.kind == TrimSourceKind::Domain &&
                 source.source.piece.curve == "periodic0" &&
                 std::min(source.span.first, source.span.last) >= 0 &&
                 std::max(source.span.first, source.span.last) <= 4;
        }));
        const auto bounded = detail::make_sheet_curve(edge.curve);
        for (int i = 0; i <= 20; ++i) {
          const auto point = bounded.curve->Value(bounded.native_range.first +
              (bounded.native_range.last - bounded.native_range.first) * i / 20.0);
          Geom2dAPI_ProjectPointOnCurve projection(point, support.curve, 0, 4);
          CHECK(projection.NbPoints() > 0);
          if (projection.NbPoints() > 0) {
            CHECK(projection.LowerDistance() < 1.0e-9);
          }
        }
      }
    }
  }
  CHECK(bounded_splines > 0);
}

void concave_corner_union() {
  area_context = "concave hole corner union";
  Region plate = rectangle_region(0, 0, 20, 10);
  plate.loops.push_back(rectangle_region(4, 2, 6, 6).loops.front());
  SheetTrimSpec spec;
  spec.operation = SheetTrimOperation::Union;
  spec.domains = {{{SheetDomainKind::Panel, "F7", 0}, {named(plate, "plate")}, {}}};
  auto line = [](gp_Pnt2d a, gp_Pnt2d b) {
    Segment segment;
    segment.kind = SegmentKind::Line;
    segment.start = a;
    segment.end = b;
    return segment;
  };
  Region chamfer{{}, {{{line({4, 2}, {6, 2}), line({6, 2}, {4, 5}), line({4, 5}, {4, 2})}}}};
  spec.tools = {named(chamfer, "chamfer")};
  const auto beveled = sheet_trim_domain(spec);
  CHECK(beveled.domains[0].regions.size() == 1);
  CHECK(beveled.domains[0].regions[0].loops.size() == 2);
  CHECK_NEAR_TOL(total_area(beveled), 200 - 36 + 3, 1.0e-10);
  check_lineage(beveled);
  Segment arc;
  arc.kind = SegmentKind::Arc;
  arc.center = {6, 4};
  arc.radius = 2;
  arc.start_angle = -kPi / 2;
  arc.end_angle = -kPi;
  Region round{{}, {{{line({4, 2}, {6, 2}), arc, line({4, 4}, {4, 2})}}}};
  spec.tools = {named(round, "round")};
  const auto rounded = sheet_trim_domain(spec);
  CHECK(rounded.domains[0].regions.size() == 1);
  CHECK(rounded.domains[0].regions[0].loops.size() == 2);
  CHECK_NEAR_TOL(total_area(rounded), 200 - 36 + 4 - kPi, 1.0e-10);
  check_lineage(rounded);
}

void overlapping_relief_tools() {
  area_context = "touching and overlapping relief tools";
  SheetTrimSpec spec;
  spec.domains = {rectangle(0, -30, 0, 30, 20)};
  spec.tools = {named(rectangle_region(-5, 3, 15, 13), "main"),
                named(rectangle_region(-8, 1, 18, 2), "lower"),
                named(rectangle_region(-8, 16, 18, 2), "upper")};
  const auto touching = sheet_trim_domain(spec);
  CHECK(touching.domains[0].regions.size() == 1);
  CHECK(touching.domains[0].regions[0].loops.size() == 1);
  CHECK_NEAR_TOL(total_area(touching), 600 - 5 * 13 - 2 * 8 * 2, 1.0e-10);
  check_lineage(touching);
  // Extend both reliefs into the main cutter. The set difference must not
  // subtract their intersection twice or leave an internal cutter boundary.
  spec.tools[1] = named(rectangle_region(-8, 1, 18, 3), "lower");
  spec.tools[2] = named(rectangle_region(-8, 15, 18, 3), "upper");
  const auto overlap = sheet_trim_domain(spec);
  CHECK(overlap.domains[0].regions.size() == 1);
  CHECK(overlap.domains[0].regions[0].loops.size() == 1);
  CHECK_NEAR_TOL(total_area(overlap), 600 - 5 * 13 - 2 * (8 * 3 - 5), 1.0e-10);
  check_lineage(overlap);
  // A second coincident contour is another proven alias of the same cut.
  spec.tools.push_back(named(rectangle_region(-8, 1, 18, 3), "alias"));
  const auto duplicate = sheet_trim_domain(spec);
  CHECK_NEAR_TOL(total_area(duplicate), total_area(overlap), 1.0e-10);
  check_lineage(duplicate);
  bool both_aliases = false;
  for (const auto& region : duplicate.domains[0].regions) {
    for (const auto& loop : region.loops) {
      for (const auto& edge : loop.edges) {
        bool lower = false;
        bool alias = false;
        for (const auto& source : edge.lineage) {
          lower |= source.source.piece.curve.compare(0, 5, "lower") == 0;
          alias |= source.source.piece.curve.compare(0, 5, "alias") == 0;
        }
        both_aliases |= lower && alias;
      }
    }
  }
  CHECK(both_aliases);
}

void invalid_seams_and_cancellation() {
  area_context = "invalid inputs and cancellation";
  auto spec = l_sheet();
  spec.seams[0].second.axial.scale = -10;
  CHECK(throws_with([&] { sheet_trim_domain(spec); }, "axial interval"));
  spec = l_sheet();
  spec.domains[1].to_operation.origin = {1, 0};
  CHECK(throws_with([&] { sheet_trim_domain(spec); }, "operation chart"));
  spec = l_sheet();
  spec.seams.push_back(spec.seams.front());
  spec.seams.back().token = 3;
  CHECK(throws_with([&] { sheet_trim_domain(spec); }, "more than one seam"));
  spec = l_sheet();
  spec.domains[0].regions[0].loops[0].edges[1].curve.geometry.start.SetY(0.01);
  CHECK(throws_with([&] { sheet_trim_domain(spec); }, "gap beyond tolerance"));
  spec = l_sheet();
  spec.tolerance = std::numeric_limits<double>::quiet_NaN();
  CHECK(throws_with([&] { sheet_trim_domain(spec); }, "positive finite tolerance"));
  spec = l_sheet();
  const auto before = total_area(sheet_trim_domain(spec));
  CancelFlag flag;
  flag.cancel();
  {
    CancelScope scope(flag);
    CHECK(throws_with([&] { sheet_trim_domain(spec); }, "cancelled"));
  }
  CHECK_NEAR_TOL(total_area(sheet_trim_domain(spec)), before, 1.0e-12);
}

} // namespace

MITCAD_BUNDLED_TEST(sheet_trim) {
  test::guarded("sheet circle crosses both seams", circle_across_two_seams);
  test::guarded("sheet trim holes islands ownership", holes_islands_and_ownership);
  test::guarded("sheet trim exact conics rational splines", exact_supports_and_reflection);
  test::guarded("sheet trim bounded periodic rational splines", periodic_spline_transport);
  test::guarded("sheet trim concave corner union", concave_corner_union);
  test::guarded("sheet trim overlapping relief tools", overlapping_relief_tools);
  test::guarded("sheet trim invalid inputs and cancellation", invalid_seams_and_cancellation);
  if (test::failures == 0) {
    std::puts("test_sheet_trim: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
