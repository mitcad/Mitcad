// SPDX-License-Identifier: MIT
#pragma once

// Minimal test helpers for the geometry tests: plain checks keep the build
// free of test frameworks.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <exception>
#include <string>
#include <vector>

#include "mitcad/geometry/geometry.hpp"

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

// Runs a test function; an exception it throws counts as a failure.
template <class F>
void guarded(const char* name, F&& f) {
  try {
    f();
  } catch (const std::exception& error) {
    std::fprintf(stderr, "%s: unexpected error: %s\n", name, error.what());
    ++failures;
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
void thread_tests();
// Sweeps, lofts, pipes, coils, ribs and webs (F3).
void sweeps_tests();
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

} // namespace test
