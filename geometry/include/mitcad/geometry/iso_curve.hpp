// SPDX-License-Identifier: MIT
#pragma once

// Isoparametric curves (mitcad#2522): the curves of a face along which one
// of its surface's two parameters keeps one value, as 3D curves. Lengths
// are millimetres. The shape is left as it is.

#include <optional>
#include <string>
#include <vector>

#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/sketch_ref.hpp"

namespace mitcad::geometry {

// The surface parameter a curve holds: U (the curve runs along v: on a
// cylinder a line along its axis) or V (it runs along u: on a cylinder a
// circle about its axis).
enum class IsoParameter { U, V };

// One isoparametric curve: where it lies (`fraction` of the face's range of
// the held parameter, 0 at its least and 1 at its greatest, and the
// surface's `parameter` there) and its pieces inside the face, in order
// along it.
struct IsoCurve {
  double fraction = 0.0;
  double parameter = 0.0;
  std::vector<ModelCurve> pieces;
};

// The isoparametric curves of the face `face` of `shape` (every face the
// name finds, over the range of all of them) along which `held` keeps one
// value: one at each of `fractions` of the face's range of that parameter,
// or, with `through`, the one through the face's point nearest to it (the
// fractions are then not used). A curve's pieces are where it runs inside
// the face (a hole or a notch of the face cuts it), as curves_of gives an
// edge's curve: the surface's own iso curve (a cylinder's exact line or
// circle, a B-spline face's own B-spline), other curves as B-splines within
// 1e-6 mm. At 0 and 1 the curve runs along the face's boundary. Throws for
// a name that finds no face, a fraction outside 0 to 1 or none at all, and
// a curve that runs nowhere inside the face.
std::vector<IsoCurve> iso_curves(const Shape& shape, const std::string& face, IsoParameter held,
                                 const std::vector<double>& fractions, const std::optional<gp_Pnt>& through);

} // namespace mitcad::geometry
