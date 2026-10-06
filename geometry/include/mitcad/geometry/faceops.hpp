// SPDX-License-Identifier: MIT
#pragma once

// Operations on named faces of a body: shell, draft, offset (press pull),
// delete and replace. Failures that come from an option the
// geometry kernel lacks throw with a message starting "unsupported:".

#include <optional>
#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/tool.hpp"

namespace mitcad::geometry {

// Removed faces (none: a closed void), and the wall thickness inside and
// outside the body's surface (one may be zero). Rounded joins outer corners
// with arcs (a rounded offset), else they stay sharp.
struct ShellSpec {
  std::vector<std::string> faces;
  double inside = 0.0;
  double outside = 0.0;
  bool tangent_chain = true;
  bool rounded = false;
};

// Hollows the body. The outside of the wall keeps the names of the body's
// faces; the new faces are named after `feature`:
//   <feature>:offset(<face>)      the inside of the wall along a face
//   <feature>:offset_cap(<face>)  the end of the wall where a removed face was
//                                 (offset_cap(<edge>) when it comes from an edge)
//   <feature>:offset(<edge>), <feature>:offset(<vertex>)  rounded joins
ShapePtr shell(const std::string& feature, const Shape& body, const ShellSpec& spec);

// Tilts the faces about their intersection with the fixed plane by `angle`.
// The pull direction is the plane's normal, or for a face of a body the
// direction into the material (against the face's outward normal), reversed
// by `flip`; a positive angle removes material on the pull side. With
// `angle2` the faces are split at the fixed plane and their other side
// tilts by `angle2` against the pull direction (symmetric and two-angle
// drafts). The faces keep their names; split pieces get "#k".
struct DraftSpec {
  std::vector<std::string> faces;
  Tool plane; // Plane or Face
  double angle = 0.0;
  std::optional<double> angle2;
  bool flip = false;
  bool tangent_chain = true;
};

ShapePtr draft(const std::string& feature, const Shape& body, const DraftSpec& spec);

// Moves the faces along their normals by `distance` (positive out of the
// material); neighbouring faces extend or shorten along their own surfaces.
// The faces keep their names.
ShapePtr offset_faces(const std::string& feature, const Shape& body,
                      const std::vector<std::string>& faces, double distance);

// Removes the faces and heals the body by extending the neighbouring faces
// until they close the gap; fails when they cannot.
ShapePtr delete_faces(const std::string& feature, const Shape& body,
                      const std::vector<std::string>& faces);

// Replaces the faces with the target (a plane, a face of a body or all
// faces of a body): the neighbours extend or shorten along their own
// surfaces to it. Planar faces onto a plane or a planar face move there;
// otherwise the region between the faces and the target, closed by the
// neighbours continued along their surfaces and by the target continued
// along its own (one face), is added to the body or removed from it. The
// new face is <feature>:replace(<face>) (pieces "#k"); the neighbours keep
// their names.
//
// Mitcad's own rules: a face between the
// target on both sides goes to the nearer side (the smaller region), and a
// region that would reach past the body's other faces does not count.
ShapePtr replace_faces(const std::string& feature, const Shape& body,
                       const std::vector<std::string>& faces, const Tool& target,
                       bool tangent_chain = true);

} // namespace mitcad::geometry
