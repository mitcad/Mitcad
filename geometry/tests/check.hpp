// SPDX-License-Identifier: MIT
#pragma once

// Minimal test helpers for the geometry tests: plain checks keep the build
// free of test frameworks.

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <string>
#include <vector>

#include "mitcad/geometry/geometry.hpp"

// windows.h (which OCCT headers such as OSD_Parallel.hxx bring in on Windows)
// defines near and far as empty macros, which would break near() below in
// every test that includes an OCCT header first.
#ifdef near
#undef near
#endif
#ifdef far
#undef far
#endif

namespace test {

inline int failures = 0;

inline void check(bool condition, const char* expression, const char* file, int line) {
  if (!condition) {
    std::fprintf(stderr, "%s:%d: check failed: %s\n", file, line, expression);
    ++failures;
  }
}

#define CHECK(expr) ::test::check((expr), #expr, __FILE__, __LINE__)

inline constexpr double kPi = 3.14159265358979323846;

inline bool near(double a, double b, double relative = 1e-6) {
  return std::abs(a - b) <= relative * std::max(1.0, std::abs(b));
}

// A check of near(a, b, relative) that prints both values when it fails.
inline void check_near(double a, double b, double relative, const char* expression,
                       const char* file, int line) {
  if (!near(a, b, relative)) {
    std::fprintf(stderr, "%s:%d: check failed: %s: %.9g != %.9g\n", file, line, expression, a, b);
    ++failures;
  }
}

#define CHECK_NEAR(a, b) ::test::check_near((a), (b), 1e-6, #a " ~ " #b, __FILE__, __LINE__)
#define CHECK_NEAR_TOL(a, b, relative)                                                             \
  ::test::check_near((a), (b), (relative), #a " ~ " #b, __FILE__, __LINE__)

// Runs a test function; an exception it throws counts as a failure. With
// MITCAD_TEST_ONLY set, only the tests whose names contain it run. A test
// that takes a second or more prints its time, so that a run's output
// shows where a part's time goes (mitcad#870).
template <class F>
void guarded(const char* name, F&& f) {
  std::string only;
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, "MITCAD_TEST_ONLY") == 0 && value != nullptr) {
    only = value;
    std::free(value);
  }
#else
  if (const char* value = std::getenv("MITCAD_TEST_ONLY")) {
    only = value;
  }
#endif
  if (!only.empty() && std::string(name).find(only) == std::string::npos) {
    return;
  }
  const auto start = std::chrono::steady_clock::now();
  try {
    f();
  } catch (const std::exception& error) {
    std::fprintf(stderr, "%s: unexpected error: %s\n", name, error.what());
    ++failures;
  }
  const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  if (seconds >= 1.0) {
    std::printf("%s: %.1f s\n", name, seconds);
  }
}

template <class F>
bool throws_with(F&& f, const char* text) {
  try {
    f();
  } catch (const std::exception& error) {
    if (std::string(error.what()).find(text) != std::string::npos) {
      return true;
    }
    std::fprintf(stderr, "unexpected error: %s\n", error.what());
    return false;
  }
  std::fprintf(stderr, "no error, expected: %s\n", text);
  return false;
}

using mitcad::geometry::Region;
using mitcad::geometry::Segment;
using mitcad::geometry::SegmentKind;
using mitcad::geometry::ShapePtr;

// Segment keys as the Rust sketch names them: line i of a rectangle whose
// lines are c<first>..c<first+3> ends at the previous and the next line.
std::string rectangle_segment(int first, int i);
std::string rectangle_region_name(int first);

// A rectangle with lines c<first>..c<first + 3> counter-clockwise from (x, y).
Region rectangle(int first, double x, double y, double width, double height);
// A circle c<curve>.
Region circle(int curve, double cx, double cy, double radius);

// Extrudes the regions on the XY plane along +Z from z = start to z = end.
ShapePtr extrude(const std::string& feature, std::vector<Region> regions, double start, double end);

std::string side(const std::string& feature, int first, int i);
std::string start_cap(const std::string& feature, int first);
std::string end_cap(const std::string& feature, int first);
std::string edge(const std::string& a, const std::string& b);

// Removed by a fillet of radius r along one straight 90-degree edge, per unit length.
inline double fillet_loss(double r) { return r * r - kPi * r * r / 4.0; }

bool has_name(const mitcad::geometry::NameList& names, const std::string& name);
// Number of faces that carry exactly this name.
int faces_named(const mitcad::geometry::Shape& shape, const std::string& name);
// Every edge between two named faces has a name, and no two edges share one.
bool edge_names_unique(const mitcad::geometry::Shape& shape);

// The folder of geometry cases kept outside the repository
// (MITCAD_GEOMETRY_CASES: bodies cut from reported models, which are not
// committed here); empty when it is not set.
std::string geometry_cases();

// A body of a geometry case read from its B-rep file, its faces named as
// imported by `feature`; null when the file cannot be read.
ShapePtr case_body(const std::string& path, const std::string& feature);

void naming_tests();
void profile_tests();
void extrude_tests();
void boolean_tests();
void dressup_tests();
void query_tests();
void transform_tests(); // F4: transforms, patterns, combine, primitives
// Face operations (F2).
void faceops_tests();
void split_tests();
void datum_tests();
// Profile features (F1).
void extrude_feature_tests();
void revolve_tests();
void hole_tests();
// Modelled threads, in three parts of about equal time (mitcad#870).
void thread_tests_1();
void thread_tests_2();
void thread_tests_3();
// Sweeps, lofts, pipes, coils, ribs and webs (F3), in five parts of about
// equal time (mitcad#870).
void sweeps_tests_1();
void sweeps_tests_2();
void sweeps_tests_3();
void sweeps_tests_4();
void sweeps_tests_5();
// Sketch text (P3).
void text_tests();
// Lofts with end conditions and rails, curve and surface fitting (P2).
void skin_tests();
// Shapes with their names as bytes (P7d).
void persist_tests();
// Long operations stopped on request (P7e).
void cancel_tests();
// Operations leave their inputs as they are (T0e).
void input_check_self_tests();
void input_check_tests();
// Booleans on perforated bodies (mitcad#72).
void far_features_tests();
// The material a near copy of a body lacks (mitcad#85).
void removed_tests();
// A body joined with a near copy (mitcad#88).
void mirror_join_tests();
// Roundings wider than a neighbouring face (mitcad#121).
void fillet_overflow_tests();
// Roundings OCCT's fillet failed on, with the port's patches (mitcad#122).
void fillet_kernel_tests();
// Ends, seams and corner caps of OCCT's roundings (mitcad#133).
void fillet_end_tests();
// Allocations that fail inside OCCT, with the port's patch (mitcad#132).
void allocation_tests();
// ShapeFix on faces with many holes, with the port's patch 0060 (mitcad#60).
void shapefix_tests();
// A chamfer whose pieces meet at a vertex of the edge they reach, with the
// port's patches 0090 and 0091 (mitcad#160).
void chamfer_kernel_tests();
// Shells whose wall makes a narrow face vanish, with the port's patch 0100
// (mitcad#172).
void shell_vanishing_tests();
// Walked roundings that are true rolling balls, with the port's patches
// 0075-0077 (mitcad#121).
void fillet_rolling_ball_tests();
// The faces a run of features made, copied by a pattern.
void face_copy_tests();
// Operations on bodies of thousands of faces, with the port's patch 0110
// (mitcad#121).
void large_body_tests();
// Corners where a rounding ends, with the port's patches 0130 and later
// (mitcad#236).
void fillet_corner_tests();
// Chamfers wider than a face beside them, with the port's patches 0140 and
// later (mitcad#240).
void chamfer_overflow_tests();
// Roundings whose results were invalid or whose computation failed, with
// the port's patches 0150 and later (mitcad#239).
void fillet_result_tests();
// Fillet contours that stop where their edges turn, with the port's patch
// 0180 (mitcad#241).
void fillet_chain_tests();
// The tangent chains of a body's edges and faces (mitcad#290).
void tangent_chain_tests();
// Roundings that run over cut-outs of their faces (mitcad#311).
void fillet_cutout_tests();
// Section curves of sketches (mitcad#389).
void section_curve_tests();
// Roundings cut by a face of the solid they pass through, with the port's
// patch 0700 (mitcad#343).
void fillet_pierced_tests();
// Faces moved by a rigid motion, their neighbours following (mitcad#343).
void move_faces_tests();
// A planar face of a body extruded as a profile (mitcad#535).
void face_extrude_tests();
// A planar face of a body revolved as a profile (mitcad#539).
void face_revolve_tests();
// Fillets and chamfers that take a face away exactly (mitcad#1106).
void dressup_takes_face_tests();
// Section Analysis' clip of a threaded revolved body in its profile's plane
// (mitcad#1230).
void section_clip_tests();
// Shells of bodies with parts no thicker than the wall (mitcad#1233).
void shell_thin_parts_tests();
// Outlines of faces seen along a direction: Project of a curved face
// (mitcad#1265).
void face_outline_tests();

} // namespace test
