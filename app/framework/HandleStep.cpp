// SPDX-License-Identifier: MIT
#include "HandleStep.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <initializer_list>

#include <BRepGProp.hxx>
#include <GProp_GProps.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>

namespace mitcad {
namespace {

// A geometry's size in steps (a 10 mm edge: 0.1 mm), and a value's (a
// 0.4 mm radius: 0.02 mm).
constexpr double kScaleSteps = 100.0;
constexpr double kValueSteps = 20.0;

} // namespace

double niceHandleStep(double size) {
  const double power = std::pow(10.0, std::floor(std::log10(std::max(size, 1e-6))));
  for (const double factor : {1.0, 2.0, 5.0, 10.0}) {
    // A size that is a step itself (0.02 as 0.4 / 20) is that step, also
    // a rounding below it.
    if (factor * power >= size * (1.0 - 1e-9)) {
      return factor * power;
    }
  }
  return 10.0 * power;
}

double handleStep(const HandleStepInput& input) {
  const double zoomed = niceHandleStep(input.zoom);
  double size = 0.0;
  if (std::isfinite(input.scale) && input.scale > 0.0) {
    size = input.scale / kScaleSteps;
  } else if (std::isfinite(input.value) && input.value != 0.0) {
    size = std::abs(input.value) / kValueSteps;
  }
  if (!(size > 0.0)) {
    return zoomed;
  }
  if (std::isfinite(input.finest) && input.finest > 0.0) {
    size = std::max(size, input.finest);
  }
  return std::min(zoomed, niceHandleStep(size));
}

double shapeScale(const TopoDS_Shape& shape) {
  if (shape.IsNull()) {
    return 0.0;
  }
  try {
    GProp_GProps lines;
    BRepGProp::LinearProperties(shape, lines);
    const double perimeter = lines.Mass();
    if (TopExp_Explorer(shape, TopAbs_FACE).More()) {
      GProp_GProps surface;
      BRepGProp::SurfaceProperties(shape, surface);
      const double area = surface.Mass();
      return area > 0.0 && perimeter > 0.0 ? 4.0 * area / perimeter : 0.0;
    }
    return perimeter > 0.0 ? perimeter : 0.0;
  } catch (const Standard_Failure&) {
    return 0.0;
  } catch (const std::exception&) {
    return 0.0;
  }
}

} // namespace mitcad
