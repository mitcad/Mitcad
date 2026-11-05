// SPDX-License-Identifier: MIT
// Geometry facade tests with real OCCT; see check.hpp.

#include <cstdlib>
#include <fstream>
#include <iterator>
#include <set>
#include <sstream>

#include "check.hpp"
#include "mitcad/geometry/import.hpp"

namespace test {

std::string rectangle_segment(int first, int i) {
  const auto curve = [first](int k) { return "c" + std::to_string(first + (k + 4) % 4); };
  return curve(i) + "[" + curve(i - 1) + "," + curve(i + 1) + "]";
}

std::string rectangle_region_name(int first) {
  std::string name = "r{";
  for (int i = 0; i < 4; ++i) {
    name += (i == 0 ? "" : ",") + rectangle_segment(first, i);
  }
  return name + "}";
}

Region rectangle(int first, double x, double y, double width, double height) {
  Region region = mitcad::geometry::rectangle_region(x, y, width, height);
  region.name = rectangle_region_name(first);
  for (int i = 0; i < 4; ++i) {
    region.loops[0].segments[static_cast<std::size_t>(i)].name = rectangle_segment(first, i);
  }
  return region;
}

Region circle(int curve, double cx, double cy, double radius) {
  Region region = mitcad::geometry::circle_region(cx, cy, radius);
  region.loops[0].segments[0].name = "c" + std::to_string(curve);
  region.name = "r{c" + std::to_string(curve) + "}";
  return region;
}

ShapePtr extrude(const std::string& feature, std::vector<Region> regions, double start, double end) {
  mitcad::geometry::ExtrudeSpec spec;
  spec.feature = feature;
  spec.regions = std::move(regions);
  spec.start = start;
  spec.end = end;
  return mitcad::geometry::extrude(spec);
}

std::string side(const std::string& feature, int first, int i) {
  return mitcad::geometry::face_name(feature, "side", rectangle_segment(first, i));
}

std::string start_cap(const std::string& feature, int first) {
  return mitcad::geometry::face_name(feature, "start", rectangle_region_name(first));
}

std::string end_cap(const std::string& feature, int first) {
  return mitcad::geometry::face_name(feature, "end", rectangle_region_name(first));
}

std::string edge(const std::string& a, const std::string& b) {
  return mitcad::geometry::make_edge_name(a, b);
}

bool has_name(const mitcad::geometry::NameList& names, const std::string& name) {
  return std::find(names.begin(), names.end(), name) != names.end();
}

int faces_named(const mitcad::geometry::Shape& shape, const std::string& name) {
  int count = 0;
  for (int i = 0; i < shape.face_count(); ++i) {
    count += has_name(shape.face_names(i), name) ? 1 : 0;
  }
  return count;
}

bool edge_names_unique(const mitcad::geometry::Shape& shape) {
  std::set<std::string> names;
  for (int i = 0; i < shape.edge_count(); ++i) {
    const auto faces = shape.edge_faces(i);
    const bool named_faces = faces[0] >= 0 && faces[1] >= 0 &&
                             !shape.face_names(faces[0]).empty() &&
                             !shape.face_names(faces[1]).empty();
    if (named_faces && shape.edge_name(i).empty()) {
      std::fprintf(stderr, "edge %d between named faces has no name\n", i);
      return false;
    }
    if (!shape.edge_name(i).empty() && !names.insert(shape.edge_name(i)).second) {
      std::fprintf(stderr, "edge name used twice: %s\n", shape.edge_name(i).c_str());
      return false;
    }
    // Every name resolves back to its own edge.
    if (!shape.edge_name(i).empty() && shape.find_edges(shape.edge_name(i)) != std::vector<int>{i}) {
      std::fprintf(stderr, "edge name does not resolve to its edge: %s\n",
                   shape.edge_name(i).c_str());
      return false;
    }
  }
  return true;
}

std::string geometry_cases() {
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

ShapePtr case_body(const std::string& path, const std::string& feature) {
  std::ifstream in(path, std::ios::binary);
  if (!in) {
    return nullptr;
  }
  std::ostringstream data;
  data << in.rdbuf();
  const std::string bytes = data.str();
  return mitcad::geometry::import_body(mitcad::geometry::read_brep_data(bytes.data(), bytes.size()), feature, 0);
}

} // namespace test

namespace {

// The suite's parts, in the order the whole run takes them. Each is a
// ctest of its own, geometry.suite.<name> (geometry/CMakeLists.txt, whose
// list of the parts geometry.suite_parts checks against this one), so
// that the parts run beside each other and the test runner profiles each
// (mitcad#832). A new part: a line at the end here and its name at the
// end of geometry_suite_parts.
struct Part {
  const char* name;
  void (*run)();
};

const Part kParts[] = {
    {"naming", test::naming_tests},
    {"profile", test::profile_tests},
    {"extrude", test::extrude_tests},
    {"boolean", test::boolean_tests},
    {"dressup", test::dressup_tests},
    {"query", test::query_tests},
    {"transform", test::transform_tests},
    // Face operations (F2).
    {"faceops", test::faceops_tests},
    {"split", test::split_tests},
    {"datum", test::datum_tests},
    {"extrude_feature", test::extrude_feature_tests},
    {"revolve", test::revolve_tests},
    {"hole", test::hole_tests},
    // Modelled threads, in three parts that run beside each other (mitcad#870).
    {"thread.1", test::thread_tests_1},
    {"thread.2", test::thread_tests_2},
    {"thread.3", test::thread_tests_3},
    // Sweeps, lofts, pipes, coils, ribs and webs (F3), in five parts that run
    // beside each other (mitcad#870).
    {"sweeps.1", test::sweeps_tests_1},
    {"sweeps.2", test::sweeps_tests_2},
    {"sweeps.3", test::sweeps_tests_3},
    {"sweeps.4", test::sweeps_tests_4},
    {"sweeps.5", test::sweeps_tests_5},
    {"text", test::text_tests},
    // Lofts with end conditions and rails, curve and surface fitting (P2).
    {"skin", test::skin_tests},
    // Shapes with their names as bytes (P7d).
    {"persist", test::persist_tests},
    // Long operations stopped on request (P7e).
    {"cancel", test::cancel_tests},
    // Operations leave their inputs as they are (T0e); the check mode's own
    // tests run first in the process that runs this part (main).
    {"input_check", test::input_check_tests},
    // Booleans on perforated bodies (mitcad#72).
    {"far_features", test::far_features_tests},
    // The material a near copy of a body lacks (mitcad#85).
    {"removed", test::removed_tests},
    // A mirror image joined only where it differs from the body (mitcad#88).
    {"mirror_join", test::mirror_join_tests},
    // Roundings wider than a neighbouring face (mitcad#121).
    {"fillet_overflow", test::fillet_overflow_tests},
    // Roundings OCCT's fillet failed on, with the port's patches (mitcad#122).
    {"fillet_kernel", test::fillet_kernel_tests},
    // Ends, seams and corner caps of OCCT's roundings (mitcad#133).
    {"fillet_ends", test::fillet_end_tests},
    // Allocations that fail inside OCCT, with the port's patch (mitcad#132).
    {"allocation", test::allocation_tests},
    // ShapeFix on faces with many holes, with the port's patch 0060 (mitcad#60).
    {"shapefix", test::shapefix_tests},
    // A chamfer whose pieces meet at a vertex of the edge they reach, with the
    // port's patches 0090 and 0091 (mitcad#160).
    {"chamfer_kernel", test::chamfer_kernel_tests},
    // Shells whose wall makes a narrow face vanish, with the port's patch 0100
    // (mitcad#172).
    {"shell_vanishing", test::shell_vanishing_tests},
    // Walked roundings that are true rolling balls, with the port's patches
    // 0075-0077 (mitcad#121).
    {"fillet_rolling_ball", test::fillet_rolling_ball_tests},
    // The faces a run of features made, copied by a pattern.
    {"face_copy", test::face_copy_tests},
    // Operations on bodies of thousands of faces, with the port's patch 0110
    // (mitcad#121).
    {"large_bodies", test::large_body_tests},
    // Corners where a rounding ends, with the port's patches 0130 and later
    // (mitcad#236).
    {"fillet_corners", test::fillet_corner_tests},
    // Chamfers wider than a face beside them, with the port's patches 0140 and
    // later (mitcad#240).
    {"chamfer_overflow", test::chamfer_overflow_tests},
    // Roundings whose results were invalid or whose computation failed, with
    // the port's patches 0150 and later (mitcad#239).
    {"fillet_results", test::fillet_result_tests},
    // Fillet contours that stop where their edges turn, with the port's patch
    // 0180 (mitcad#241).
    {"fillet_chains", test::fillet_chain_tests},
    // The tangent chains of a body's edges and faces (mitcad#290).
    {"tangent_chain", test::tangent_chain_tests},
    // Roundings that run over cut-outs of their faces (mitcad#311).
    {"fillet_cutouts", test::fillet_cutout_tests},
    // Section curves of sketches (mitcad#389).
    {"section_curves", test::section_curve_tests},
    // Roundings cut by a face of the solid they pass through, with the port's
    // patches 0700 and 0933 (mitcad#343).
    {"fillet_pierced", test::fillet_pierced_tests},
    // Faces moved by a rigid motion, their neighbours following (mitcad#343).
    {"move_faces", test::move_faces_tests},
    // A planar face of a body extruded as a profile (mitcad#535).
    {"face_extrude", test::face_extrude_tests},
    // A planar face of a body revolved as a profile (mitcad#539).
    {"face_revolve", test::face_revolve_tests},
    // Fillets and chamfers that take a face away exactly (mitcad#1106).
    {"dressup_takes_face", test::dressup_takes_face_tests},
    // Section Analysis' clip of a threaded revolved body in its profile's
    // plane (mitcad#1230).
    {"section_clip", test::section_clip_tests},
    // Shells of bodies with parts no thicker than the wall (mitcad#1233).
    {"shell_thin_parts", test::shell_thin_parts_tests},
    // Outlines of faces seen along a direction: Project of a curved face
    // (mitcad#1265).
    {"face_outline", test::face_outline_tests},
};

bool is_part(const std::string& name) {
  return std::any_of(std::begin(kParts), std::end(kParts),
                     [&name](const Part& part) { return name == part.name; });
}

int usage() {
  std::fprintf(stderr, "usage: test_geometry [--part NAME...] | --list | --check-parts NAME...\n");
  return 2;
}

// --check-parts: whether the names (the ctests' list) are every part, each
// once; prints the differences.
int check_parts(const std::vector<std::string>& names) {
  int wrong = 0;
  std::set<std::string> seen;
  for (const std::string& name : names) {
    if (!is_part(name)) {
      std::fprintf(stderr, "geometry_suite_parts names %s, which is no part of test_geometry\n", name.c_str());
      ++wrong;
    } else if (!seen.insert(name).second) {
      std::fprintf(stderr, "geometry_suite_parts names %s twice\n", name.c_str());
      ++wrong;
    }
  }
  for (const Part& part : kParts) {
    if (seen.count(part.name) == 0) {
      std::fprintf(stderr, "the part %s is not in geometry_suite_parts (geometry/CMakeLists.txt), so no ctest runs it\n",
                   part.name);
      ++wrong;
    }
  }
  if (wrong == 0) {
    std::printf("test_geometry: the %zu parts are the ctests' parts\n", seen.size());
  }
  return wrong == 0 ? 0 : 1;
}

} // namespace

// Without arguments every part runs, in one process; --part NAME... runs
// only those (the ctests geometry.suite.<name>).
int main(int argc, char** argv) {
  std::vector<std::string> args(argv + 1, argv + argc);
  if (args.size() == 1 && args[0] == "--list") {
    for (const Part& part : kParts) {
      std::puts(part.name);
    }
    return 0;
  }
  if (!args.empty() && args[0] == "--check-parts") {
    return check_parts(std::vector<std::string>(args.begin() + 1, args.end()));
  }
  std::set<std::string> selected;
  if (!args.empty()) {
    if (args[0] != "--part" || args.size() < 2) {
      return usage();
    }
    for (std::size_t i = 1; i < args.size(); ++i) {
      if (!is_part(args[i])) {
        std::fprintf(stderr, "test_geometry: no part %s (--list names them)\n", args[i].c_str());
        return usage();
      }
      selected.insert(args[i]);
    }
  }
  const bool all = selected.empty();
  // Every operation of the run is checked for changing its inputs (T0e).
  mitcad::geometry::enable_input_checks(true);
  // The check mode's own tests clear what they made, so they come first.
  if (all || selected.count("input_check") > 0) {
    test::input_check_self_tests();
  }
  for (const Part& part : kParts) {
    if (all || selected.count(part.name) > 0) {
      part.run();
    }
  }
  for (const std::string& change : mitcad::geometry::input_changes()) {
    std::fprintf(stderr, "%s\n", change.c_str());
  }
  CHECK(mitcad::geometry::input_changes().empty());
  if (test::failures == 0) {
    std::puts("test_geometry: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
