// SPDX-License-Identifier: MIT
// Repeated automatic-operation contact queries on synthetic geometry.
#include <algorithm>
#include <array>
#include <chrono>
#include <cstdio>
#include <memory>
#include <thread>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Builder.hxx>
#include <NCollection_List.hxx>
#include <TopoDS_Compound.hxx>
#include <TopLoc_Location.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax1.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"
#include "mitcad/geometry/contact.hpp"
#include "mitcad/io/mesh.hpp"
#include "bundled_test.hpp"

namespace {

using namespace mitcad::geometry;

Shape perforated_target() {
  BRep_Builder builder;
  TopoDS_Compound holes;
  builder.MakeCompound(holes);
  int count = 0;
  for (int x = 8; x <= 108; x += 10) {
    for (int y = 8; y <= 88; y += 10) {
      if (x > 45 && x < 80 && y > 35 && y < 70) { continue; }
      builder.Add(holes, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x, y, -1), gp_Dir(0, 0, 1)), 2, 22).Shape());
      ++count;
    }
  }
  CHECK(count == 83);
  BRepAlgoAPI_Cut cut;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(BRepPrimAPI_MakeBox(120, 100, 20).Shape());
  cut.SetArguments(arguments);
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(holes);
  cut.SetTools(tools);
  cut.SetRunParallel(true);
  cut.SetNonDestructive(true);
  cut.Build();
  CHECK(cut.IsDone());
  Shape target(cut.Shape());
  // The existing target is already displayed before a drag starts. The
  // changing tool has not yet been meshed when automatic contacts run.
  mitcad::io::MeshOptions mesh;
  mesh.linear_deflection = 0.01;
  mesh.angular_deflection = 5 * test::kPi / 180;
  mesh.face_triangles = 20000;
  mitcad::io::mesh_in_place(target.occt(), mesh);
  return target;
}

Shape tool(double start, double distance) {
  return Shape(BRepPrimAPI_MakeBox(gp_Pnt(50, 45, std::min(start, start + distance)),
                                  20, 10, std::abs(distance)).Shape());
}

void distance_tests(const Shape& target) {
  const Shape upper(BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 35), 120, 100, 20).Shape());
  const std::vector<const Shape*> bodies{&target, &upper};
  for (int repeat = 0; repeat < 3; ++repeat) {
    CHECK(tool_contacts(tool(25, -4), bodies) == std::vector<ToolContact>({ToolContact::Apart, ToolContact::Apart}));
    CHECK(tool_contacts(tool(25, -5), bodies) == std::vector<ToolContact>({ToolContact::Touches, ToolContact::Apart}));
    CHECK(tool_contacts(tool(25, -15), bodies) == std::vector<ToolContact>({ToolContact::Inside, ToolContact::Apart}));
    CHECK(tool_contacts(tool(25, 10), bodies) == std::vector<ToolContact>({ToolContact::Apart, ToolContact::Touches}));
    CHECK(tool_contacts(tool(25, 30), bodies) == std::vector<ToolContact>({ToolContact::Apart, ToolContact::Inside}));
  }
  gp_Trsf placement;
  placement.SetTranslation(gp_Vec(200, 0, 0));
  const Shape moved(target.occt().Moved(TopLoc_Location(placement)));
  CHECK(tool_contacts(tool(20, 5), {&moved}) == std::vector<ToolContact>({ToolContact::Apart}));
  const Shape replacement(BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), 120, 100, 10).Shape());
  CHECK(tool_contacts(tool(20, 5), {&replacement}) == std::vector<ToolContact>({ToolContact::Apart}));
  CHECK(tool_contacts(tool(20, 5), {&target}) == std::vector<ToolContact>({ToolContact::Touches}));
  gp_Trsf rotation;
  rotation.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0)), 0.7);
  const Shape rotated(target.occt().Moved(TopLoc_Location(rotation)));
  const Shape rotated_tool(tool(20, 5).occt().Moved(TopLoc_Location(rotation)));
  CHECK(tool_contacts(rotated_tool, {&rotated}) == std::vector<ToolContact>({ToolContact::Touches}));
  // A point on the infinite cap plane is not a contact when its projection
  // is in a hole. The trimmed-face witness must reject this candidate.
  const Shape in_hole(BRepPrimAPI_MakeBox(gp_Pnt(7.75, 7.75, 20), 0.5, 0.5, 5).Shape());
  CHECK(tool_contacts(in_hole, {&target}) == std::vector<ToolContact>({ToolContact::Apart}));
  CHECK(tool_contacts(tool(20 + 0.5e-6, 5), {&target}) == std::vector<ToolContact>({ToolContact::Touches}));
  CHECK(tool_contacts(tool(20 + 2e-6, 5), {&target}) == std::vector<ToolContact>({ToolContact::Apart}));
}

void concurrent_tests(const Shape& target) {
  std::array<int, 4> answers{};
  std::vector<std::thread> workers;
  for (int i = 0; i < 4; ++i) {
    workers.emplace_back([&target, &answers, i] {
      const Shape candidate = tool(25, i % 2 == 0 ? -5 : -15);
      const ToolContact expected = i % 2 == 0 ? ToolContact::Touches : ToolContact::Inside;
      bool correct = true;
      for (int repeat = 0; repeat < 5; ++repeat) {
        correct = correct && tool_contacts(candidate, {&target}) == std::vector<ToolContact>({expected});
      }
      answers[static_cast<std::size_t>(i)] = correct;
    });
  }
  for (auto& worker : workers) { worker.join(); }
  CHECK(std::all_of(answers.begin(), answers.end(), [](bool correct) { return correct; }));
}

void repeated_query_comparison(const Shape& target) {
  constexpr int samples = 21;
  std::vector<std::unique_ptr<Shape>> fresh;
  std::vector<std::unique_ptr<Shape>> tools;
  for (int i = 0; i < samples; ++i) {
    fresh.push_back(std::make_unique<Shape>(target.occt()));
    tools.push_back(std::make_unique<Shape>(tool(20, 5 + i)));
  }
  std::vector<double> retained_times;
  std::vector<double> fresh_times;
  const auto measure = [](const Shape& candidate, const Shape& body) {
    const auto start = std::chrono::steady_clock::now();
    const auto result = tool_contacts(candidate, {&body});
    const auto end = std::chrono::steady_clock::now();
    CHECK(result == std::vector<ToolContact>({ToolContact::Touches}));
    return std::chrono::duration<double, std::milli>(end - start).count();
  };
  // Alternate ordering, with wrapper construction and target meshing
  // outside the measured queries. Geometry and quality are identical.
  for (int i = 0; i < samples; ++i) {
    if (i % 2 == 0) {
      retained_times.push_back(measure(*tools[i], target));
      fresh_times.push_back(measure(*tools[i], *fresh[i]));
    } else {
      fresh_times.push_back(measure(*tools[i], *fresh[i]));
      retained_times.push_back(measure(*tools[i], target));
    }
  }
  const auto median = [](std::vector<double> values) {
    std::sort(values.begin(), values.end());
    return values[values.size() / 2];
  };
  std::printf("Contact comparison: samples=%d retained_median_ms=%.6f fresh_median_ms=%.6f target_faces=%d\n",
              samples, median(retained_times), median(fresh_times), target.face_count());
}

} // namespace

MITCAD_BUNDLED_TEST(tool_contacts) {
  const Shape target = perforated_target();
  distance_tests(target);
  const Shape concurrent_target(target.occt());
  concurrent_tests(concurrent_target);
  repeated_query_comparison(target);
  // A remesh changes display quality, not the immutable contact geometry.
  mitcad::io::MeshOptions coarse;
  coarse.linear_deflection = 0.2;
  coarse.angular_deflection = 20 * test::kPi / 180;
  coarse.face_triangles = 1000;
  mitcad::io::mesh_in_place(target.occt(), coarse);
  distance_tests(target);
  return test::failures == 0 ? 0 : 1;
}
