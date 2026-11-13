// SPDX-License-Identifier: MIT
// The default 3D view (mitcad#1698, view/DefaultView.hpp): Home, a design
// read into a new design's Top and Create Sketch's plane pick in a new
// design look a few degrees off the isometric view. Seen through OCCT's
// orthographic camera as the 3D view sets it (that direction, Z up), the
// origin planes' squares in the Centred style (100 mm, centred on the
// origin: Preferences › Display › Origin planes with Style Centred and Size
// Model at its default, mitcad#1748; the default Corner style's squares lie
// each in its own positive quadrant, so they never overlap, and Size Screen
// only scales them about the origin):
//   - edges of two planes along different axes are at least 5° apart on
//     the screen;
//   - edges of two planes along the same axis, parallel in any parallel
//     projection, do not lie on top of each other: their lines are at
//     least 4 % of the side apart on the screen;
//   - each plane is seen at least half open, none near edge-on.
// Along the exact cube diagonal the second check fails (the planes were
// hard to tell apart), and View › Isometric stays that exact diagonal.
#include <algorithm>
#include <cstddef>
#include <initializer_list>
#include <cmath>
#include <cstdio>
#include <vector>

#include <Graphic3d_Camera.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "view/DefaultView.hpp"

namespace {
int failures = 0;
void check(bool value, const char* expression, int line) {
  if (!value) {
    std::fprintf(stderr, "default_view.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(value) check((value), #value, __LINE__)

constexpr double kDegree = 3.14159265358979323846 / 180.0;
constexpr double kSide = 100.0;             // MainWindow::kDefaultDatumSize, the Centred style
constexpr double kMinAngle = 5.0;           // degrees between edges along different axes
constexpr double kMinApart = 0.04 * kSide; // between the lines of parallel edges

// An edge of an origin plane's square: its ends, the axis it runs along
// (0 X, 1 Y, 2 Z) and its plane (the axis of the plane's normal).
struct Edge {
  gp_Pnt from;
  gp_Pnt to;
  int axis;
  int plane;
};

gp_XYZ unit(int axis) {
  return gp_XYZ(axis == 0 ? 1.0 : 0.0, axis == 1 ? 1.0 : 0.0, axis == 2 ? 1.0 : 0.0);
}

// The four edges of each of XY, XZ and YZ.
std::vector<Edge> originPlaneEdges() {
  std::vector<Edge> edges;
  const double half = kSide / 2.0;
  for (int normal = 0; normal < 3; ++normal) {
    const int u = (normal + 1) % 3;
    const int v = (normal + 2) % 3;
    for (const int along : {u, v}) {
      const int across = along == u ? v : u;
      for (const double side : {-half, half}) {
        const gp_XYZ offset = unit(across) * side;
        edges.push_back({gp_Pnt(offset - unit(along) * half), gp_Pnt(offset + unit(along) * half), along, normal});
      }
    }
  }
  return edges;
}

// The 3D view's orthographic camera looking along `direction`, Z up.
occ::handle<Graphic3d_Camera> viewCamera(const gp_Dir& direction) {
  occ::handle<Graphic3d_Camera> camera = new Graphic3d_Camera();
  camera->SetProjectionType(Graphic3d_Camera::Projection_Orthographic);
  camera->SetAspect(1.0);
  camera->SetEyeAndCenter(gp_Pnt(gp_Vec(direction).Reversed().XYZ() * 500.0), gp_Pnt(0.0, 0.0, 0.0));
  camera->SetUp(mitcad::defaultViewUp());
  camera->OrthogonalizeUp();
  return camera;
}

// A point on the screen, in mm across the view (right) and up.
struct ScreenPoint {
  double x;
  double y;
};

ScreenPoint onScreen(const Graphic3d_Camera& camera, const gp_Pnt& point) {
  const gp_Vec up(camera.Up());
  const gp_Vec right = gp_Vec(camera.Direction()).Crossed(up);
  const gp_Vec at(point.XYZ());
  return {at.Dot(right), at.Dot(up)};
}

// How the planes' edges lie on the screen: the smallest angle between edges
// of two planes along different axes, and the smallest distance between
// the lines of edges of two planes along the same axis.
struct Layout {
  double minAngle = 180.0; // degrees
  double minApart = 1.0e9; // mm on the screen
};

Layout layoutOf(const gp_Dir& direction) {
  const occ::handle<Graphic3d_Camera> camera = viewCamera(direction);
  const std::vector<Edge> edges = originPlaneEdges();
  Layout layout;
  for (std::size_t i = 0; i < edges.size(); ++i) {
    for (std::size_t j = i + 1; j < edges.size(); ++j) {
      const Edge& a = edges[i];
      const Edge& b = edges[j];
      if (a.plane == b.plane) {
        continue;
      }
      const ScreenPoint a0 = onScreen(*camera, a.from);
      const ScreenPoint a1 = onScreen(*camera, a.to);
      const ScreenPoint b0 = onScreen(*camera, b.from);
      const ScreenPoint b1 = onScreen(*camera, b.to);
      const double ax = a1.x - a0.x;
      const double ay = a1.y - a0.y;
      const double bx = b1.x - b0.x;
      const double by = b1.y - b0.y;
      const double lengthA = std::hypot(ax, ay);
      const double lengthB = std::hypot(bx, by);
      if (a.axis == b.axis) {
        // Parallel: the distance of b's line from a's.
        const double apart = std::abs(ax * (b0.y - a0.y) - ay * (b0.x - a0.x)) / lengthA;
        layout.minApart = std::min(layout.minApart, apart);
      } else {
        const double sine = std::abs(ax * by - ay * bx) / (lengthA * lengthB);
        layout.minAngle = std::min(layout.minAngle, std::asin(std::min(1.0, sine)) / kDegree);
      }
    }
  }
  return layout;
}

void defaultDirectionTests() {
  const gp_Dir direction = mitcad::defaultViewDirection();
  const Layout layout = layoutOf(direction);
  CHECK(layout.minAngle >= kMinAngle);
  CHECK(layout.minApart >= kMinApart);
  // Every plane seen at least half open: XY, XZ and YZ.
  CHECK(std::abs(direction.Z()) >= 0.45);
  CHECK(std::abs(direction.Y()) >= 0.45);
  CHECK(std::abs(direction.X()) >= 0.45);
  // The same feel as the isometric view: from the front right and above,
  // a few degrees off its diagonal.
  CHECK(direction.X() < 0.0 && direction.Y() > 0.0 && direction.Z() < 0.0);
  const double offDiagonal = direction.Angle(mitcad::isometricViewDirection()) / kDegree;
  CHECK(offDiagonal >= 3.0 && offDiagonal <= 10.0);
  CHECK(mitcad::defaultViewUp().IsEqual(gp_Dir(0.0, 0.0, 1.0), 1.0e-12));
  // The direction the azimuth and elevation give.
  CHECK(std::abs(std::asin(-direction.Z()) / kDegree - mitcad::kDefaultViewElevation) <= 1.0e-9);
  CHECK(std::abs(std::atan2(-direction.X(), direction.Y()) / kDegree - mitcad::kDefaultViewAzimuth) <= 1.0e-9);
}

void isometricTests() {
  // View › Isometric: exactly along the cube's diagonal.
  const gp_Dir iso = mitcad::isometricViewDirection();
  const double third = 1.0 / std::sqrt(3.0);
  CHECK(std::abs(iso.X() + third) <= 1.0e-12);
  CHECK(std::abs(iso.Y() - third) <= 1.0e-12);
  CHECK(std::abs(iso.Z() + third) <= 1.0e-12);
  // There the planes' parallel edges lie on one line pairwise: the check
  // above tells the two views apart.
  const Layout layout = layoutOf(iso);
  CHECK(layout.minApart <= 1.0e-9);
  CHECK(layout.minAngle >= kMinAngle);
}

} // namespace

int defaultViewTests() {
  defaultDirectionTests();
  isometricTests();
  return failures;
}
