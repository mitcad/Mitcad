// SPDX-License-Identifier: MIT
#pragma once

// The step a dragged arrow handle rounds its value to (mitcad#1431,
// app/COMMANDS.md "withManipulator"): the zoom's step, about three pixels
// of the view, or a finer one from the size of the geometry the handle
// works on, so that a fillet on a small component's edge moves in steps
// that fit the edge without a zoom in. Plain math, unit tested
// (app/tests/handle_step.cpp); the sizes of shapes are read with OCCT.

#include <TopoDS_Shape.hxx>

namespace mitcad {

// 1, 2 or 5 times a power of ten: the smallest such step at least `size`
// long (a size below 1e-6 as 1e-6).
double niceHandleStep(double size);

// What an arrow's step follows, each in the value's unit (mm for a
// length).
struct HandleStepInput {
  // Three pixels of the view along the arrow: the zoom's step is
  // niceHandleStep(zoom), as before mitcad#1431.
  double zoom = 0.0;
  // The size of the geometry the handle works on (Manipulator::scale): its
  // step is niceHandleStep(scale / 100), 0.1 mm for a 10 mm edge. 0: none.
  double scale = 0.0;
  // Without a scale, the value when the drag began, when it may count as
  // one: niceHandleStep(|value| / 20), 0.02 mm for a 0.4 mm radius. 0: none.
  double value = 0.0;
  // The last decimal the value box shows: a step from the scale or the
  // value is not finer. 0: no floor.
  double finest = 0.0;
};

// The zoom's step, or the finer one the scale (else the value) gives,
// never below `finest`. Never coarser than the zoom's step: without a
// scale and a value, or with a large one, it is the zoom's step itself.
double handleStep(const HandleStepInput& input);

// The size of a shape as a handle's scale reads it: a face's (or a
// profile's, several faces' together) 4 area / perimeter, which is the side
// of a square, the diameter of a disc and about twice the width of a long
// strip; else the length of its edges (a curve, a wire). 0 when it has
// neither.
double shapeScale(const TopoDS_Shape& shape);

} // namespace mitcad
