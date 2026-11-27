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

// A face whose wall has a thickness of its own (mitcad#2443): `inside` and
// `outside` in place of the shell's, each greater than zero exactly where
// the shell's is.
struct ShellFace {
  std::string face;
  double inside = 0.0;
  double outside = 0.0;
};

// Removed faces (none: a closed void), and the wall thickness inside and
// outside the body's surface (one may be zero). Rounded joins outer corners
// with arcs (a rounded offset), else they stay sharp. `own` are faces whose
// wall has a thickness of its own; the rest keep the shell's.
struct ShellSpec {
  std::vector<std::string> faces;
  double inside = 0.0;
  double outside = 0.0;
  bool tangent_chain = true;
  bool rounded = false;
  std::vector<ShellFace> own;
};

// Hollows the body. The outside of the wall keeps the names of the body's
// faces; the new faces are named after `feature`:
//   <feature>:offset(<face>)      the inside of the wall along a face
//   <feature>:offset_cap(<face>)  the end of the wall where a removed face was
//                                 (offset_cap(<edge>) when it comes from an edge)
//   <feature>:offset(<edge>), <feature>:offset(<vertex>)  rounded joins
// Where the geometry kernel's offset fails, the inside wall is the body less
// a cavity (mitcad#1233): the parts of the body no thicker than the wall
// (under a face no further from a removed face than the wall, a plate
// between two faces no further apart than twice the wall) stay solid with
// their own faces, and roundings tighter than the wall are run past. The
// result's notes name the faces whose inside of the wall is missing: "the
// wall does not fit under face …; it is left solid there", or "the inside
// of the wall runs past face …". Where the wall fits under no face, the
// shell fails with that message.
//
// Faces with a thickness of their own (mitcad#2443) are offset by it in the
// geometry kernel's own offset (a thickness for each face). The kernel
// offsets faces that meet smoothly as one, and in a rounded shell also the
// faces a rounding joins where the rounding cannot run from one thickness
// to the other (it can along a straight edge where neither face turns,
// along a circle about whose axis both turn, and round a corner of three
// faces square to each other; mitcad#2488): such faces take one thickness.
// With the tangent chain the faces tangent to one with its own thickness
// take it too; a face joined so to one of another thickness is refused,
// the faces painted. The
// cavity above is not built for such a shell (it has one thickness): where
// the kernel's offset fails, the shell fails, and where it succeeds at the
// shell's thickness alone, the error paints the faces whose own thickness
// the neighbouring walls cannot meet.
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
// The faces keep their names. Where a moved face continues smoothly into a
// face that stays (a piece of a split face beside the other pieces, or a
// face tangent to it) and the neighbours cannot follow, the seam becomes a
// step (mitcad#1928): the faces it runs into smoothly are offset with it,
// and only the region at the moved faces, cut off at the seams along their
// normals, is removed from the body or added to it. The step's faces are
// <feature>:step(<moved face>) ("#k" for several); where the step cannot
// be made the offset fails saying so. With `tangent_chain` (mitcad#2438)
// the faces joined to the listed ones through smooth edges move with them,
// grown as replace_faces grows its faces. Planar faces in one plane
// (mitcad#2548) end on the level of another planar face of the body where
// they come within 1e-6 mm of it and cross the body's other faces as Move
// Face's do (move_faces_rigidly): flush with the side of the base under a
// raised block's end, or past it, the overhang's underside named
// <feature>:step(<moved face>).
ShapePtr offset_faces(const std::string& feature, const Shape& body,
                      const std::vector<std::string>& faces, double distance,
                      bool tangent_chain = false);

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
