// SPDX-License-Identifier: MIT
#pragma once

// Replace face for faces and targets of any surface type (P5): the region
// between the replaced faces and the target, closed by the neighbouring
// faces continued along their surfaces, is added to or removed from the
// body. Internal to the geometry library; faceops.cpp keeps its own path
// for planar faces and planar targets.

#include <exception>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/tool.hpp"

namespace mitcad::geometry::detail {

// Replaces the faces (indices into the body) with the target (a plane, a
// face of another body or another body's faces) and names the new face
// `name` (pieces "#k"); the neighbours keep their names. Without
// `name_walls` the region's faces on the neighbours' continued surfaces
// get no names of their own: where the region's face merges with the
// neighbour it continues, the merged face has the neighbour's names, and
// one that faces the other way (the underside of an overhang) stays
// unnamed for the caller to name (mitcad#2548).
ShapePtr replace_by_slab(const Shape& body, const std::vector<int>& faces, const Tool& target,
                         const std::string& name, bool name_walls = true);

// Planar faces moved along their normal across the body's other faces
// (mitcad#2548): a raised block's end moved flush with the side of the
// base it stands on, or past it. The faces' own rebuild (Move Face's
// move_faces_rigidly, Offset Face's offset of the body) keeps the body's
// topology: the neighbour under the block would have to lose a boundary
// there, so it cannot. Here the region between the faces and their new
// place, closed by the neighbouring faces continued along their surfaces
// (replace_by_slab's cells), is added to the body (a move out of the
// material) or removed from it (into it), and the faces around are
// trimmed where they meet or cross.

// How near two parallel planes must lie to be one level (mm): a face moved
// within this of the level of another planar face of the body lands on it
// (a typed distance rounded in its last digits).
constexpr double kPlanarLevel = 1.0e-6;

// The plane the faces lie in when the move along their normal applies:
// all planar, in one plane (within kPlanarLevel), facing one way, and
// `motion` along that plane's normal (either way). Null otherwise.
std::optional<gp_Pln> movable_plane(const Shape& body, const std::vector<int>& faces, const gp_Vec& motion);

// `motion` made to end exactly on the level of a planar face of the body
// (not one of `faces`) parallel to `plane` where it ends within
// kPlanarLevel of it (exactly there too); null where it ends on no such
// level. Faces that land on another face's level may join it, which their
// own rebuild cannot do: the callers try move_planar_faces first there.
std::optional<gp_Vec> landing_motion(const Shape& body, const std::vector<int>& faces, const gp_Pln& plane,
                                     const gp_Vec& motion);

// The faces, all in `plane` (movable_plane), moved by `motion` along its
// normal through the region they sweep. The moved faces keep their names
// (a face that joins a coplanar face of the body holds both faces' names;
// pieces "#k"); the neighbours keep theirs; a face of the region that
// continues no face of the body (the underside of an overhang) is
// "<feature>:step(<face>)" after the first moved face. Throws when the
// region cannot be built or added, or the result is not a valid solid.
ShapePtr move_planar_faces(const Shape& body, const std::vector<int>& faces, const gp_Pln& plane,
                           const gp_Vec& motion, const std::string& feature);

// The faces of a feature (a boss, a pocket, a rib) moved rigidly as a whole
// where the faces around cannot follow them along their surfaces
// (mitcad#2632, GitHub mitcad/Mitcad#769): a boss slid partly over the edge
// of the plate it stands on, whose top would have to lose a boundary there.
// The faces are removed (the faces around extended over where they were,
// as Delete Face does), the region between them and those extended faces
// (the boss above the plate's top, or a pocket's hollow below it) is moved
// by `motion` and added to the body again (or removed from it), and the
// faces around are trimmed where they meet. The moved faces and the faces
// around keep their names; a face of the moved region that is no face of
// the body (the underside of the part of a boss that overhangs) is
// "<feature>:step(<face>)" after the first moved face. Throws NotAFeature
// where the faces around cannot close over the faces (they bound no
// feature), and a reason in words where the moved region would no longer
// meet the rest of the body, a moved face would end up inside it, or the
// result is not one valid solid.
class NotAFeature : public std::runtime_error {
public:
  using std::runtime_error::runtime_error;
};
ShapePtr move_feature_faces(const Shape& body, const std::vector<int>& faces, const gp_Trsf& motion,
                            const std::string& feature);

// The result of `a`, or where `a` fails, of `b`: the faces' own rebuild and
// move_planar_faces, in the order that suits the motion. Where both fail,
// the failure of `a` (reported 0) or of `b` (1) is thrown. A cancel request
// is never caught.
template <class A, class B>
ShapePtr first_of(A&& a, B&& b, int reported) {
  std::exception_ptr failures[2];
  try {
    return a();
  } catch (const Cancelled&) {
    throw;
  } catch (const std::exception&) {
    failures[0] = std::current_exception();
  }
  try {
    return b();
  } catch (const Cancelled&) {
    throw;
  } catch (const std::exception&) {
    failures[1] = std::current_exception();
  }
  std::rethrow_exception(failures[reported == 0 ? 0 : 1]);
}

} // namespace mitcad::geometry::detail
