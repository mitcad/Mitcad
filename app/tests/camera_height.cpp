// SPDX-License-Identifier: MIT
// A stored view's height and OCCT's camera scale (mitcad#959): the height a
// camera shows (ViewDimensions, what OcctViewer::camera() stores for named
// views and Home) set back with setVisibleHeight (view/CameraHeight.hpp,
// what OcctViewer::setCamera does) shows the same extent again, in views
// wider and taller than wide and in both projections. Setting the height
// as the scale, as setCamera did before, showed a view taller than wide
// zoomed out by 1/aspect.
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <limits>

#include <Graphic3d_Camera.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_XYZ.hxx>

#include "view/CameraHeight.hpp"

namespace {
int failures = 0;
void check(bool value, const char* expression, int line) {
  if (!value) {
    std::fprintf(stderr, "camera_height.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(value) check((value), #value, __LINE__)

bool nearly(double a, double b) { return std::abs(a - b) <= 1e-9 * std::max(std::abs(a), std::abs(b)); }

// A camera as the 3D view has one: `aspect` wide for its height, looking
// along +Y at the origin with +Z up.
occ::handle<Graphic3d_Camera> viewCamera(double aspect, bool perspective) {
  occ::handle<Graphic3d_Camera> camera = new Graphic3d_Camera();
  camera->SetProjectionType(perspective ? Graphic3d_Camera::Projection_Perspective
                                        : Graphic3d_Camera::Projection_Orthographic);
  camera->SetAspect(aspect);
  camera->SetEyeAndCenter(gp_Pnt(0.0, -400.0, 0.0), gp_Pnt(0.0, 0.0, 0.0));
  camera->SetUp(gp_Dir(0.0, 0.0, 1.0));
  camera->OrthogonalizeUp();
  return camera;
}

void scaleTests() {
  using mitcad::cameraScaleForHeight;
  // Wider than tall the scale is the height, taller than wide the width.
  CHECK(cameraScaleForHeight(120.0, 1.6) == 120.0);
  CHECK(cameraScaleForHeight(120.0, 1.0) == 120.0);
  CHECK(nearly(cameraScaleForHeight(120.0, 0.6), 72.0));
  // An aspect that makes no view leaves the height as it is.
  CHECK(cameraScaleForHeight(120.0, 0.0) == 120.0);
  CHECK(cameraScaleForHeight(120.0, -2.0) == 120.0);
  CHECK(cameraScaleForHeight(120.0, std::numeric_limits<double>::quiet_NaN()) == 120.0);
  CHECK(cameraScaleForHeight(120.0, std::numeric_limits<double>::infinity()) == 120.0);
}

void cameraTests() {
  for (const double aspect : {0.6, 1.6}) {
    for (const bool perspective : {false, true}) {
      // A view as the user left it, its height read as camera() reads it.
      occ::handle<Graphic3d_Camera> saved = viewCamera(aspect, perspective);
      saved->SetScale(50.0);
      const double scale = saved->Scale();
      const gp_XYZ extent = saved->ViewDimensions();
      const double height = extent.Y();
      CHECK(scale > 0.0 && height > 0.0);
      // Taller than wide the height is larger than the scale: the case of
      // mitcad#959.
      CHECK(aspect < 1.0 ? nearly(height, scale / aspect) : nearly(height, scale));

      // Restored on a camera of the same aspect that shows something else:
      // the same scale and the same extent come back.
      occ::handle<Graphic3d_Camera> restored = viewCamera(aspect, perspective);
      restored->SetScale(400.0);
      mitcad::setVisibleHeight(*restored, height);
      CHECK(nearly(restored->Scale(), scale));
      CHECK(nearly(restored->ViewDimensions().Y(), height));
      CHECK(nearly(restored->ViewDimensions().X(), extent.X()));
      if (perspective) {
        // A perspective camera shows it from as far as the saved one.
        CHECK(nearly(restored->Distance(), saved->Distance()));
      }

      // Saved and restored again and again (Home, then a named view, …):
      // nothing grows or shrinks.
      for (int round = 0; round < 50; ++round) {
        mitcad::setVisibleHeight(*restored, restored->ViewDimensions().Y());
      }
      CHECK(nearly(restored->Scale(), scale));
      CHECK(nearly(restored->ViewDimensions().Y(), height));

      // A height stored in a document (120 mm of the model at the target)
      // shows 120 mm from top to bottom at either aspect.
      occ::handle<Graphic3d_Camera> stored = viewCamera(aspect, perspective);
      mitcad::setVisibleHeight(*stored, 120.0);
      CHECK(nearly(stored->ViewDimensions().Y(), 120.0));
      CHECK(nearly(stored->ViewDimensions().X(), 120.0 * aspect));

      // The height set as the scale, as before mitcad#959: right wider than
      // tall, zoomed out by 1/aspect taller than wide.
      occ::handle<Graphic3d_Camera> old = viewCamera(aspect, perspective);
      old->SetScale(120.0);
      CHECK(aspect < 1.0 ? nearly(old->ViewDimensions().Y(), 120.0 / aspect)
                         : nearly(old->ViewDimensions().Y(), 120.0));
    }
  }
}

} // namespace

int cameraHeightTests() {
  scaleTests();
  cameraTests();
  return failures;
}
