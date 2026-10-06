// SPDX-License-Identifier: MIT
#pragma once

// Asymmetric and curvature continuous (G2) fillets (P6): a chamfer whose
// bevels are replaced by blend surfaces. OCCT's fillets have circular
// cross-sections only; its chamfers give the contact lines on the faces,
// the ends and the topology, so only the bevels' surfaces change.
// Internal to the geometry library.

#include <map>
#include <string>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// What a bevel's cross-sections become: a conic tangent to the faces (the
// affine image of the circular arc inscribed in their angle), or with
// `curvature` a quintic that also continues the faces' curvature, its
// shape set by the tangency weight (0.1 to 2).
struct BlendSection {
  bool curvature = false;
  double weight = 1.0;
};

// The bevel faces of `chamfered` (a chamfer of `body` whose bevels are
// named <feature>:fillet(<edge>), edges of `body`) replaced by surfaces
// through cross-sections of the kind `sections` gives for each bevelled
// edge, in planes across the edge, between the chamfer's contact lines.
// Where a chain of bevels ends, the face it ends on must be planar: the
// cross-section there lies in it, and that face takes the curve instead of
// the bevel's straight end. Bevels around a closed chain of edges join
// each other; a bevel around one closed edge closes on itself. Along a
// circle between faces of revolution about its axis (ends in planes
// through it), the surface is the cross-section turned about the axis;
// elsewhere a Gordon surface through cross-sections along the edge.
ShapePtr blend_bevels(const std::string& feature, const Shape& body, const Shape& chamfered,
                      const std::map<std::string, BlendSection>& sections);

} // namespace mitcad::geometry::detail
