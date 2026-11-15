// SPDX-License-Identifier: MIT
#pragma once

// Press Pull on a face continued as a profile (mitcad#1850): the face
// carried Straight along its own normal, or Along the walls, turning with
// them, with the new piece's walls drafted or its profile scaled over the
// run. Offset, the face moved with its neighbours following along their own
// surfaces, is offset_faces (faceops.hpp).
//
// How the walls continue (face_run): the faces beside the face (its walls)
//   - all parallel to one direction not in the face's plane (planes
//     containing it, cylinders and extrusions along it): the face is
//     carried along that direction, unturned;
//   - all of revolution about one axis (tori, cones, cylinders and surfaces
//     of revolution about it, spheres centred on it, planes square to it):
//     the face is revolved about that axis, turning with the walls, so that
//     it stays square to the run;
//   - otherwise the walls do not continue, and Along fails in words.
// The distance is the length of the path of the face's area centre: an arc
// length for a turn. A turn has no limit at the walls' inner radius; it
// may go round to a closed ring, and fails in words where it would run into
// the body itself or come round to the face.
//
// Tapers widen or narrow the new piece over the run (outward positive):
//   Draft  each wall leans by the angle from its edge on the face, as an
//          extrusion's taper leans it (its outline grows, its holes shrink;
//          corners as the extrusion makes them), by t tan(angle) at run
//          length t;
//   Scale  the face's whole outline grows or shrinks about its area centre,
//          every point in proportion, from 1 at the face to `end_scale` at
//          the end, linearly with the run length.
// A positive distance adds the new piece to the body (out of the material);
// a negative one cuts it from the body (into the material).
//
// Faces are named after `feature` as an extrusion of the face is
// (face_extrude.hpp):
//   side(<edge>)   the new wall from an edge of the face, by the edge's name
//   start(<face>)  the piece's cap on the face (gone in the result)
//   end(<face>)    the moved end, which also keeps the face's own names
// `<face>` is the reference stored for the face. The body is not modified.

#include <string>
#include <vector>

#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

enum class PressPullRun { Straight, Along };
enum class PressPullTaper { None, Draft, Scale };

struct PressPullSpec {
  std::string feature;
  // References of planar faces of the body; each continues on its own.
  std::vector<std::string> faces;
  PressPullRun run = PressPullRun::Straight;
  // Signed, mm: positive out of the material (joined), negative into it
  // (cut). Not zero.
  double distance = 0.0;
  PressPullTaper taper = PressPullTaper::None;
  // Draft: radians, between -85 and 85 degrees; positive widens.
  double taper_angle = 0.0;
  // Scale: the end's size to the face's, more than zero (1.1: 110 %).
  double end_scale = 1.0;
};

ShapePtr press_pull(const Shape& body, const PressPullSpec& spec);

// How a planar face's walls continue (see above), for Along and for the
// application's handle.
struct FaceRun {
  // The face's area centre and its normal out of the material.
  gp_Pnt centre;
  gp_Dir normal;
  // How far the outline reaches from the area centre at its farthest point
  // (Scale's angle there, for the application).
  double reach = 0.0;
  enum class Along { None, Straight, Turn };
  Along along = Along::None;
  // Straight: the walls' direction, out of the material.
  gp_Dir direction;
  // Turn: the walls' axis, the right hand turn about it out of the
  // material, and the area centre's distance from it.
  gp_Ax1 axis;
  double radius = 0.0;
  // None: why, as an error a person reads (user_error.hpp: the walls that
  // stop the run as its places).
  std::string refusal;
};

// Throws when the reference finds no face or several, or the face is not
// planar.
FaceRun face_run(const Shape& body, const std::string& face);

} // namespace mitcad::geometry
