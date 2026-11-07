// SPDX-License-Identifier: MIT
// Real material reconstruction: a circular cut crosses both L tangent seams.
#include <array>
#include <map>
#include <set>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRep_Tool.hxx>
#include <Geom_BSplineSurface.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopTools_ShapeMapHasher.hxx>

#include "check.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/sheet_trim.hpp"

namespace {
using namespace mitcad::geometry;
constexpr double kBA = 1.8 * test::kPi;

MaterialRegion rectangle(double x, double y, double width, double height, const std::string& feature) {
  MaterialRegion result;
  MaterialLoop loop;
  const auto region = rectangle_region(x, y, width, height);
  int id = 0;
  for (const auto& s : region.loops.front().segments) {
    loop.edges.push_back({{feature + ":sheet_curve(" + std::to_string(id++) + ")", 0}, {s, {0, 1}}});
  }
  result.loops.push_back(loop);
  return result;
}

const std::array<SheetDomainRef, 3> domains{{
    {SheetDomainKind::Panel, "F3", 0}, {SheetDomainKind::Bend, "F4", 0}, {SheetDomainKind::Panel, "F4", 0}}};

ShapePortSide port_side(const SheetPatch& patch, std::size_t index) {
  const auto& edge = patch.regions.front().loops.front().edges[index];
  const auto& s = edge.curve.geometry;
  return {{patch.domain, edge.piece}, edge.curve.range, {s.start.Y(), s.end.Y() - s.start.Y()}};
}

SheetChartShapeSpec uncut_l(bool opened = false) {
  SheetChartShapeSpec spec;
  spec.thickness = 2;
  const PlaneEmbedding base{{{0, 20, -2}, {1, 0, 0}, {0, -1, 0}}, MaterialParity::Negative};
  const CylinderEmbedding bend{{0, 20, 3}, {0, -1, 0}, {0, 0, -1}, {1, 0, 0}, 3, 3.6, test::kPi / 2, false};
  const PlaneEmbedding flange{{{5, 20, 3 - kBA}, {0, 0, 1}, {0, -1, 0}}, MaterialParity::Negative};
  spec.patches.push_back({domains[0], {rectangle(-30, 0, 30, 20, "F10")}, base});
  spec.patches.push_back({domains[1], {rectangle(0, 0, kBA, 20, "F11")}, bend});
  spec.patches.push_back({domains[2], {rectangle(kBA, 0, 15, 20, "F12")}, flange});
  if (opened) for (auto& patch : spec.patches) patch.embedding = base;
  spec.ports.push_back({0, {0, 20}, port_side(spec.patches[0], 1), port_side(spec.patches[1], 3)});
  spec.ports.push_back({1, {0, 20}, port_side(spec.patches[1], 1), port_side(spec.patches[2], 3)});
  return spec;
}

SheetChartShapeSpec cut_l(bool opened, bool circle, double radius = 4) {
  SheetChartShapeSpec spec = uncut_l(opened);
  SheetTrimSpec trim;
  for (const auto& patch : spec.patches) trim.domains.push_back({patch.domain, patch.regions, {}});
  for (const auto& port : spec.ports) {
    trim.seams.push_back({port.token,
                         {port.first.boundary, port.first.span, port.first.axial},
                         {port.second.boundary, port.second.span, port.second.axial}, port.axial_interval});
  }
  if (circle) {
    Segment s; s.kind = SegmentKind::Circle; s.center = {kBA / 2, 10}; s.radius = radius;
    MaterialLoop loop;
    loop.edges.push_back({{"F20:sheet_curve(0)", 0}, {s, {0, 2 * test::kPi}}});
    trim.tools.push_back({{loop}});
  } else trim.tools.push_back(rectangle(-2, 8, kBA + 4, 4, "F20"));
  const SheetTrimResult result = sheet_trim_domain(trim);
  CHECK(result.diagnostics.empty());
  CHECK(result.domains.size() == 3);
  CHECK(result.ports.size() == 4);
  std::map<std::pair<std::string, std::uint32_t>, MaterialEdge> material;
  const auto key = [](const SheetDomainRef& d) {
    return d.feature + (d.kind == SheetDomainKind::Panel ? ".p" : ".b") + std::to_string(d.slot);
  };
  for (auto& patch : spec.patches) {
    patch.regions.clear();
    for (const auto& domain : result.domains) if (domain.domain == patch.domain) {
      for (const auto& region : domain.regions) {
        MaterialRegion output;
        for (const auto& loop : region.loops) {
          MaterialLoop boundary;
          for (const auto& edge : loop.edges) {
            MaterialEdge settled{{"F21:sheet_curve(" + std::to_string(edge.token) + ")", 0}, edge.curve};
            material.emplace(std::make_pair(key(domain.domain), edge.token), settled);
            boundary.edges.push_back(std::move(settled));
          }
          output.loops.push_back(std::move(boundary));
        }
        patch.regions.push_back(std::move(output));
      }
    }
  }
  spec.ports.clear();
  for (const auto& port : result.ports) {
    const auto side = [&](const TrimEndpointSpan& end) {
      const auto& edge = material.at({key(end.domain), end.edge_token});
      CHECK(edge.curve.geometry.kind == SegmentKind::Line);
      const auto& s = edge.curve.geometry;
      return ShapePortSide{{end.domain, edge.piece}, end.range, {s.start.Y(), s.end.Y() - s.start.Y()}};
    };
    // Persistent identity allocation belongs to the model. This test assigns
    // one local port per returned surviving interval and retains its range.
    spec.ports.push_back({static_cast<std::uint32_t>(spec.ports.size()), port.axial_interval, side(port.first), side(port.second)});
  }
  return spec;
}

ShapePtr only(const SheetChartShapeSpec& spec) {
  const auto result = sheet_shape(spec);
  CHECK(result.diagnostics.empty());
  CHECK(result.solids.size() == 1);
  if (result.solids.size() != 1) throw std::runtime_error("expected one L sheet solid");
  const auto shape = result.solids.front().shape;
  CHECK(BRepCheck_Analyzer(shape->occt()).IsValid());
  std::set<std::string> seen;
  for (const auto& member : result.solids.front().membership) {
    seen.insert(member.domain.feature + (member.domain.kind == SheetDomainKind::Panel ? ".p" : ".b") +
                std::to_string(member.domain.slot));
  }
  CHECK(seen.size() == 3);
  for (int i = 0; i < shape->face_count(); ++i) CHECK(!shape->face_names(i).empty());
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> ancestors;
  TopExp::MapShapesAndAncestors(shape->occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  for (int i = 1; i <= ancestors.Extent(); ++i) CHECK(ancestors(i).Size() == 2);
  return shape;
}

gp_Pnt2d inverse(const gp_Pnt& p) {
  return {3.6 * std::atan2(p.X(), 3 - p.Z()), 20 - p.Y()};
}

double radius(const gp_Pnt& p) { return std::hypot(p.X(), p.Z() - 3); }

double mask_error(const gp_Pnt2d& p) { return std::abs(p.Distance({kBA / 2, 10}) - 4); }

void actual_circle_across_both_seams() {
  const auto spec = cut_l(false, true);
  const auto shape = only(spec);
  int curved_cap_edges = 0, curved_walls = 0, tangent_edges = 0;
  for (int f = 0; f < shape->face_count(); ++f) {
    const auto face = shape->face(f);
    BRepAdaptor_Surface surface(face);
    if (surface.GetType() == GeomAbs_Cylinder) {
      const double r = surface.Cylinder().Radius();
      CHECK(std::abs(r - 3) < 1e-12 || std::abs(r - 5) < 1e-12);
      for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
        BRepAdaptor_Curve edge(TopoDS::Edge(it.Current()));
        const double a = edge.FirstParameter(), b = edge.LastParameter();
        if (mask_error(inverse(edge.Value((a + b) / 2))) < 1e-7) {
          ++curved_cap_edges;
          for (int i = 0; i <= 32; ++i) {
            const auto p = edge.Value(a + (b - a) * i / 32);
            CHECK(std::abs(radius(p) - r) < 1e-7);
            CHECK(mask_error(inverse(p)) < 1e-7);
          }
        }
        const auto mid = inverse(edge.Value((a + b) / 2));
        if (std::abs(mid.X()) < 1e-7 || std::abs(mid.X() - kBA) < 1e-7) ++tangent_edges;
      }
    }
    bool bend_wall = false;
    for (const auto& name : shape->face_names(f)) if (name.find("F4:sheet_bend_side") == 0) bend_wall = true;
    if (!bend_wall) continue;
    const auto wall = occ::handle<Geom_BSplineSurface>::DownCast(BRep_Tool::Surface(face));
    if (wall.IsNull()) continue;
    double a, b, q0, q1; wall->Bounds(a, b, q0, q1);
    if (mask_error(inverse(wall->Value((a + b) / 2, q0))) >= 1e-7) continue;
    ++curved_walls;
    for (int i = 0; i <= 32; ++i) {
      const double t = a + (b - a) * i / 32;
      const auto front = wall->Value(t, q0), back = wall->Value(t, q1);
      CHECK(mask_error(inverse(front)) < 1e-7);
      CHECK(inverse(front).Distance(inverse(back)) < 1e-7);
      CHECK(std::abs(front.Distance(back) - 2) < 1e-7);
      CHECK(std::abs(radius(front) - 5) < 1e-7);
      CHECK(std::abs(radius(back) - 3) < 1e-7);
      for (int q = 1; q < 4; ++q) {
        const auto p = wall->Value(t, q0 + (q1 - q0) * q / 4);
        CHECK(mask_error(inverse(p)) < 1e-7);
        CHECK(std::abs(radius(p) - (5 - q / 2.0)) < 1e-7);
      }
    }
  }
  CHECK(curved_cap_edges >= 4);
  CHECK(curved_walls >= 2);
  CHECK(tangent_edges == 8);
  // No uncut sector remains inside the developed mask, through thickness.
  for (int i = 1; i < 10; ++i) for (double r : {3.25, 4.0, 4.75}) {
    const double angle = (kBA * i / 10) / 3.6;
    BRepClass3d_SolidClassifier classifier(shape->occt(), {r * std::sin(angle), 10, 3 - r * std::cos(angle)}, 1e-7);
    CHECK(classifier.State() == TopAbs_OUT);
  }
  const auto opened = only(cut_l(true, true));
  CHECK_NEAR_TOL(volume(*opened), 2 * (20 * (45 + kBA) - 16 * test::kPi), 1e-8);
  const auto box = bounding_box(*opened);
  CHECK_NEAR(box.min.Z(), -2); CHECK_NEAR(box.max.Z(), 0);
  const auto refolded = only(spec);
  CHECK_NEAR_TOL(volume(*refolded), volume(*shape), 1e-10);
}

void uncut_and_rectangular_cut() {
  CHECK_NEAR_TOL(volume(*only(uncut_l())), 1800 + 80 * test::kPi, 1e-8);
  CHECK_NEAR_TOL(volume(*only(uncut_l(true))), 1800 + 72 * test::kPi, 1e-8);
  CHECK_NEAR_TOL(volume(*only(cut_l(false, false))), 1800 + 80 * test::kPi - 4 * (8 + 4 * test::kPi), 1e-8);
  CHECK_NEAR_TOL(volume(*only(cut_l(true, false))), 2 * (20 * (45 + kBA) - 4 * (kBA + 4)), 1e-8);
}

void tangent_mask_and_adjacent_base_panels() {
  const auto tangent = only(cut_l(false, true, kBA / 2));
  CHECK(volume(*tangent) < 1800 + 80 * test::kPi);
  CHECK_NEAR_TOL(volume(*only(cut_l(true, true, kBA / 2))),
                 2 * (20 * (45 + kBA) - test::kPi * kBA * kBA / 4), 1e-8);
  SheetChartShapeSpec spec;
  spec.thickness = 2;
  spec.patches.push_back({domains[0], {rectangle(0, 0, 10, 20, "F30")}, PlaneEmbedding{}});
  spec.patches.push_back({domains[2], {rectangle(10, 0, 15, 20, "F31")}, PlaneEmbedding{}});
  const auto result = sheet_shape(spec);
  CHECK(result.solids.size() == 1);
  if (result.solids.size() == 1) {
    CHECK_NEAR(volume(*result.solids[0].shape), 1000);
    CHECK(result.solids[0].membership.size() == 2);
    CHECK(!result.solids[0].shape->find_faces("F3:sheet_front(0)").empty());
    CHECK(!result.solids[0].shape->find_faces("F4:sheet_front(0)").empty());
  }
}

void refusals_and_cancellation() {
  auto spec = uncut_l();
  spec.ports[0].second.axial.offset += .01;
  CHECK(test::throws_with([&] { sheet_shape(spec); }, "axial interval"));
  spec = uncut_l();
  std::get<CylinderEmbedding>(spec.patches[1].embedding).inside_radius = 0;
  CHECK(test::throws_with([&] { sheet_shape(spec); }, "radius"));
  spec = uncut_l();
  std::get<PlaneEmbedding>(spec.patches[0].embedding).front_to_back_sign = MaterialParity::Positive;
  CHECK(test::throws_with([&] { sheet_shape(spec); }, "material sides"));
  spec = uncut_l();
  spec.patches = {spec.patches[1]};
  spec.ports.clear();
  spec.patches[0].regions = {rectangle(0, 0, kBA + .01, 20, "F11")};
  CHECK(test::throws_with([&] { sheet_shape(spec); }, "outside the cylinder angle"));
  spec = uncut_l();
  CancelFlag flag; flag.cancel();
  const CancelScope scope(flag);
  CHECK(test::throws_with([&] { sheet_shape(spec); }, "cancelled"));
}

void connected_material_collision() {
  // Three quarter-turns produce a long return flange that passes through the
  // stationary base. Every declared port still meets exactly: graph closure
  // and ordinary topological validity cannot substitute for collision checks.
  auto spec = uncut_l();
  spec.patches[2].regions = {rectangle(0, 0, 15, 20, "F12")};
  spec.patches[2].embedding = PlaneEmbedding{{{5, 20, 3}, {0, 0, 1}, {0, -1, 0}}, MaterialParity::Negative};
  spec.patches.push_back({{SheetDomainKind::Bend, "F5", 0}, {rectangle(0, 0, kBA, 20, "F13")},
      CylinderEmbedding{{0, 20, 18}, {0, -1, 0}, {1, 0, 0}, {0, 0, 1}, 3, 3.6, test::kPi / 2, false}});
  spec.patches.push_back({{SheetDomainKind::Panel, "F5", 0}, {rectangle(0, 0, 20, 20, "F14")},
      PlaneEmbedding{{{0, 20, 23}, {-1, 0, 0}, {0, -1, 0}}, MaterialParity::Negative}});
  spec.patches.push_back({{SheetDomainKind::Bend, "F6", 0}, {rectangle(0, 0, kBA, 20, "F15")},
      CylinderEmbedding{{-20, 20, 18}, {0, -1, 0}, {0, 0, 1}, {-1, 0, 0}, 3, 3.6, test::kPi / 2, false}});
  spec.patches.push_back({{SheetDomainKind::Panel, "F6", 0}, {rectangle(0, 0, 30, 20, "F16")},
      PlaneEmbedding{{{-25, 20, 18}, {0, 0, -1}, {0, -1, 0}}, MaterialParity::Negative}});
  spec.ports.clear();
  for (std::size_t i = 1; i < spec.patches.size(); ++i) {
    spec.ports.push_back({static_cast<std::uint32_t>(i), {0, 20}, port_side(spec.patches[i - 1], 1), port_side(spec.patches[i], 3)});
  }
  // The refusal names the two walls that pass through each other (mitcad#1409).
  CHECK(test::throws_with([&] { sheet_shape(spec); }, "sheet walls panel F3 slot 0 and panel F6 slot 0 pass through each other"));
}

void concave_corner_union_roundtrip() {
  MaterialRegion blank = rectangle(0, 0, 40, 30, "F30");
  blank.loops.push_back(rectangle(10, 10, 10, 10, "F31").loops.front());
  MaterialRegion triangle;
  MaterialLoop loop;
  const std::array<gp_Pnt2d, 3> points{{{10, 10}, {12, 10}, {10, 13}}};
  for (std::size_t i = 0; i < points.size(); ++i) {
    Segment s; s.kind = SegmentKind::Line; s.start = points[i]; s.end = points[(i + 1) % points.size()];
    loop.edges.push_back({{"F32:sheet_curve(" + std::to_string(i) + ")", 0}, {s, {0, 1}}});
  }
  triangle.loops.push_back(loop);
  SheetTrimSpec trim;
  trim.operation = SheetTrimOperation::Union;
  trim.domains.push_back({domains[0], {blank}, {}});
  trim.tools.push_back(triangle);
  const auto result = sheet_trim_domain(trim);
  CHECK(result.domains.size() == 1);
  CHECK(result.domains[0].regions.size() == 1);
  SheetChartShapeSpec spec; spec.thickness = 2;
  SheetPatch patch{domains[0], {}, PlaneEmbedding{}};
  for (const auto& region : result.domains[0].regions) {
    MaterialRegion out;
    for (const auto& boundary : region.loops) {
      MaterialLoop bounded;
      for (const auto& edge : boundary.edges) bounded.edges.push_back({{"F33:sheet_curve(" + std::to_string(edge.token) + ")", 0}, edge.curve});
      out.loops.push_back(bounded);
    }
    patch.regions.push_back(out);
  }
  spec.patches.push_back(patch);
  const auto shape = sheet_shape(spec);
  CHECK(shape.solids.size() == 1);
  if (shape.solids.size() == 1) CHECK_NEAR(volume(*shape.solids[0].shape), 2206);
}
} // namespace

int main() {
  enable_input_checks(true);
  test::guarded("sheet L circle across both seams", actual_circle_across_both_seams);
  test::guarded("sheet L material volumes", uncut_and_rectangular_cut);
  test::guarded("sheet tangent mask and adjacent base panels", tangent_mask_and_adjacent_base_panels);
  test::guarded("sheet chart refusals", refusals_and_cancellation);
  test::guarded("sheet connected material collision", connected_material_collision);
  test::guarded("sheet concave corner union roundtrip", concave_corner_union_roundtrip);
  CHECK(input_changes().empty());
  if (test::failures == 0) std::puts("test_sheet_bend: all checks passed");
  return test::failures == 0 ? 0 : 1;
}
