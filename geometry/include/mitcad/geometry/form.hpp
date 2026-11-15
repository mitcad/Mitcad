// SPDX-License-Identifier: MIT
#pragma once

// Form bodies (mitcad#1757): the bicubic patches of a control cage's
// Catmull-Clark limit surface, which the model works out
// (core/subdiv), made faces and sewn into a solid or a sheet. Lengths are
// millimetres.

#include <string>
#include <vector>

#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// A non-rational B-spline patch: degrees, `nu` by `nv` poles (pole (i, j),
// i along u, at i * nv + j), the distinct knots of each direction with their
// multiplicities (adding up to the poles plus the degree plus one), and the
// name of its face.
struct SurfacePatch {
  std::string name;
  int u_degree = 3;
  int v_degree = 3;
  int nu = 0;
  int nv = 0;
  std::vector<gp_Pnt> poles;
  std::vector<double> u_knots;
  std::vector<int> u_mults;
  std::vector<double> v_knots;
  std::vector<int> v_mults;
};

// The patches as faces, each named by its patch's name, sewn where their
// boundaries lie within `tolerance` of each other. With `closed` they must
// close into one solid, oriented with its material inside; else the sewn
// faces are a sheet. The surfaces are kept as given. Throws for a patch
// whose counts, knots or poles do not make a B-spline surface, for no
// patches or a tolerance that is not greater than zero, and, with
// `closed`, for patches that leave edges open.
ShapePtr patch_body(const std::vector<SurfacePatch>& patches, bool closed, double tolerance);

} // namespace mitcad::geometry
