// SPDX-License-Identifier: MIT
// The import's analysis queries run as operations of the geometry library
// (mitcad#2294): with MITCAD_TEST_OCCT_CRASH=<operation> a query crashes as
// an access violation inside OCCT would, and the query fails with the error
// "<operation>: ..." instead of a crash that ends the import process.
//   test_analysis_crash boundary-distances  (MITCAD_TEST_OCCT_CRASH="boundary distances")
//     -> both calls fail with the error; the face point query, which takes
//        the same boundary index's lock, still finds its point (the crash
//        left the lock released)
//   test_analysis_crash points-inside  (MITCAD_TEST_OCCT_CRASH="points inside")
//     -> the query fails with the error; the boundary distances still work

#include <array>
#include <cmath>
#include <cstdio>
#include <exception>
#include <string>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <Bnd_Box.hxx>

#include "bridge/analysis.hpp"
#include "mitcad/geometry/guard.hpp"
#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace {

// Whether `query` fails with the error "<operation>: ...", whose text names
// no OCCT function.
template <typename Query>
bool fails_as(const char* operation, Query query) {
  try {
    query();
  } catch (const std::exception& error) {
    const std::string text = error.what();
    std::printf("%s: error: %s\n", operation, text.c_str());
    if (text.rfind(std::string(operation) + ": ", 0) != 0) {
      std::fprintf(stderr, "%s: the error does not name the operation\n", operation);
      return false;
    }
    if (text.find("::") != std::string::npos) {
      std::fprintf(stderr, "%s: the error names a function\n", operation);
      return false;
    }
    return true;
  }
  std::fprintf(stderr, "%s: the query did not fail (MITCAD_TEST_OCCT_CRASH=\"%s\" not set?)\n", operation,
               operation);
  return false;
}

} // namespace

int main(int argc, char* argv[]) {
  const std::string which = argc == 2 ? argv[1] : "";
  if (which != "boundary-distances" && which != "points-inside") {
    std::fprintf(stderr, "usage: test_analysis_crash boundary-distances|points-inside\n");
    return 2;
  }
  mitcad::geometry::catch_occt_crashes();
  const mitcad::geometry::Shape box(BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape());
  const std::vector<double> points{5.0, 5.0, 5.0, 5.0, 5.0, 12.0};
  const rust::Slice<const double> slice(points.data(), points.size());
  bool ok = true;
  if (which == "boundary-distances") {
    for (int call = 0; call < 2; ++call) {
      ok = fails_as("boundary distances", [&] { mitcad::bridge::analysis_boundary_distances(box, slice); }) && ok;
    }
    // The top face (z = 10) under a point above it.
    std::vector<int> faces;
    for (int i = 0; i < box.face_count(); ++i) {
      Bnd_Box face_box;
      BRepBndLib::Add(box.face(i), face_box);
      double x0 = 0.0;
      double y0 = 0.0;
      double z0 = 0.0;
      double x1 = 0.0;
      double y1 = 0.0;
      double z1 = 0.0;
      face_box.Get(x0, y0, z0, x1, y1, z1);
      if (std::abs(z0 - 10.0) < 1e-3 && std::abs(z1 - 10.0) < 1e-3) {
        faces.push_back(i);
      }
    }
    std::array<double, 3> point{};
    std::array<double, 3> normal{};
    if (faces.size() != 1 ||
        !mitcad::bridge::analysis_face_point(box, faces, {5.0, 5.0, 12.0}, point, normal) ||
        std::abs(point[2] - 10.0) > 1e-9) {
      std::fprintf(stderr, "face point: not found after the crash\n");
      ok = false;
    }
  } else {
    ok = fails_as("points inside", [&] { mitcad::bridge::analysis_points_inside(box, slice); });
    const rust::Vec<double> d = mitcad::bridge::analysis_boundary_distances(box, slice);
    if (d.size() != 2 || std::abs(d[0] - 5.0) > 1e-9 || std::abs(d[1] - 2.0) > 1e-9) {
      std::fprintf(stderr, "boundary distances: wrong after the crash\n");
      ok = false;
    }
  }
  return ok ? 0 : 1;
}
