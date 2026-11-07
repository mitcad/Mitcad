// SPDX-License-Identifier: MIT
// The step a dragged arrow handle rounds its value to (mitcad#1431,
// framework/HandleStep.hpp): the zoom's step (1, 2 or 5 times a power of
// ten, about three pixels), a finer one from the size of the geometry the
// handle works on (a hundredth of it) or, without one, from the value the
// drag starts from (a twentieth of it), never coarser than the zoom's step
// and never below the value box's last decimal; and the sizes of shapes
// the commands give as the scale.
#include "framework/HandleStep.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <initializer_list>

#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Shape.hxx>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "handle_step.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool nearly(double a, double b) { return std::abs(a - b) <= 1e-9 * std::max(1.0, std::abs(b)); }

double stepOf(double zoom, double scale, double value = 0.0, double finest = 0.0) {
  mitcad::HandleStepInput input;
  input.zoom = zoom;
  input.scale = scale;
  input.value = value;
  input.finest = finest;
  return mitcad::handleStep(input);
}

void niceStepTests() {
  CHECK(nearly(mitcad::niceHandleStep(1.5), 2.0));
  CHECK(nearly(mitcad::niceHandleStep(2.0), 2.0));
  CHECK(nearly(mitcad::niceHandleStep(0.3), 0.5));
  CHECK(nearly(mitcad::niceHandleStep(3.0), 5.0));
  CHECK(nearly(mitcad::niceHandleStep(7.0), 10.0));
  CHECK(nearly(mitcad::niceHandleStep(0.1), 0.1));
  CHECK(nearly(mitcad::niceHandleStep(0.0), 1e-6));
}

void stepTests() {
  // A view where three pixels are 1.5 mm (a 200 mm block and a 10 mm cube
  // framed together): the zoom's step is 2 mm.
  const double zoom = 1.5;
  // No scale: the zoom's step, as before mitcad#1431 (and as Place rounds a
  // click).
  CHECK(nearly(stepOf(zoom, 0.0), 2.0));
  CHECK(nearly(stepOf(0.3, 0.0), 0.5));
  CHECK(nearly(stepOf(0.0007, 0.0), 0.001));
  for (const double z : {0.004, 0.07, 0.6, 4.0, 40.0}) {
    CHECK(nearly(stepOf(z, 0.0), mitcad::niceHandleStep(z)));
  }
  // A scale finer than the zoom: the 10 mm cube's edge steps by 0.1 mm.
  CHECK(nearly(stepOf(zoom, 10.0), 0.1));
  CHECK(nearly(stepOf(zoom, 30.0), 0.5));
  // A scale coarser than the zoom: the zoom's step is kept (the 200 mm
  // block's edge at 2 mm, a large part zoomed in at 0.05 mm).
  CHECK(nearly(stepOf(zoom, 200.0), 2.0));
  CHECK(nearly(stepOf(zoom, 1000.0), 2.0));
  CHECK(nearly(stepOf(0.03, 200.0), 0.05));
  // A scale that is no size: the zoom's step.
  CHECK(nearly(stepOf(zoom, -5.0), 2.0));
  CHECK(nearly(stepOf(zoom, std::nan("")), 2.0));
  // Without a scale the value the drag starts from: a 0.4 mm radius by
  // 0.02 mm, also when negative; a large value keeps the zoom's step.
  CHECK(nearly(stepOf(zoom, 0.0, 0.4), 0.02));
  CHECK(nearly(stepOf(zoom, 0.0, -0.4), 0.02));
  CHECK(nearly(stepOf(zoom, 0.0, 500.0), 2.0));
  // A scale goes before the value.
  CHECK(nearly(stepOf(zoom, 200.0, 0.4), 2.0));
  CHECK(nearly(stepOf(zoom, 10.0, 0.4), 0.1));
  // The floor: a step from a tiny scale stops at the box's last decimal
  // (4 decimals of mm: 0.0001 mm; of an inch: 0.00254 mm, rounded up to
  // 0.005 mm).
  CHECK(nearly(stepOf(zoom, 0.001, 0.0, 1e-4), 1e-4));
  CHECK(nearly(stepOf(zoom, 0.01, 0.0, 2.54e-3), 0.005));
  CHECK(nearly(stepOf(zoom, 0.0, 0.0001, 1e-4), 1e-4));
  // The floor never makes the step coarser than the zoom's.
  CHECK(nearly(stepOf(0.0003, 0.001, 0.0, 0.001), 0.0005));
  // Without a floor a tiny scale goes as low as niceHandleStep does.
  CHECK(nearly(stepOf(zoom, 0.001), 1e-5));
}

// The face of `shape` whose shapeScale comes closest to `expected`.
double closestFace(const TopoDS_Shape& shape, double expected) {
  double best = -1.0;
  for (TopExp_Explorer faces(shape, TopAbs_FACE); faces.More(); faces.Next()) {
    const double size = mitcad::shapeScale(faces.Current());
    if (best < 0.0 || std::abs(size - expected) < std::abs(best - expected)) {
      best = size;
    }
  }
  return best;
}

void shapeTests() {
  // A square's side, a long strip's twice its width (4 area / perimeter).
  const TopoDS_Shape cube = BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape();
  for (TopExp_Explorer faces(cube, TopAbs_FACE); faces.More(); faces.Next()) {
    CHECK(std::abs(mitcad::shapeScale(faces.Current()) - 10.0) < 1e-6);
  }
  for (TopExp_Explorer edges(cube, TopAbs_EDGE); edges.More(); edges.Next()) {
    CHECK(std::abs(mitcad::shapeScale(edges.Current()) - 10.0) < 1e-6);
  }
  const TopoDS_Shape strip = BRepPrimAPI_MakeBox(100.0, 2.0, 50.0).Shape();
  CHECK(std::abs(closestFace(strip, 4.0) - 4.0 * 200.0 / 204.0) < 1e-6);
  // A disc's diameter: a cylinder of radius 5 has two.
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(5.0, 30.0).Shape();
  CHECK(std::abs(closestFace(cylinder, 10.0) - 10.0) < 1e-6);
  // Nothing to measure.
  CHECK(mitcad::shapeScale(TopoDS_Shape()) == 0.0);
}

} // namespace

int handleStepTests() {
  niceStepTests();
  stepTests();
  shapeTests();
  return failures;
}
