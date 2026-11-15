// SPDX-License-Identifier: MIT
#pragma once

// A taper whose leaned walls meet or vanish within the extrusion
// (mitcad#1851): a slot or a gap that a widening taper closes, a hole it
// fills, a narrow part a narrowing taper eats away. OCCT's draft leans
// each wall where it is and never changes the topology, so there its
// walls cross into an invalid shell; here the section at each height is
// the profile offset by the height times the taper's tangent with mitred
// corners (each edge's offset, the corners joined as the leaned planes of
// a draft meet), its topology changing where it does. Internal to the
// geometry library (sweep.cpp's tapered() uses it).

#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pln.hxx>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/user_error.hpp"

namespace mitcad::geometry::detail {

// The prism `solid` tapered by `angle` (see ExtrudeSide::taper: a positive
// angle moves every loop away from the material by h tan(angle) at the
// height h above the neutral plane, along `pull`) with its topology events
// resolved. `near` is the face of its cap on the neutral plane, the faces
// not `leaned` its caps.
//
// Built as the prism and the bands each wall sweeps on its way: every
// edge of the cap sweeps a wedge between its wall and the wall leaned (a
// band of the wall's offsets at each height up the prism; an arc's offset
// toward its centre ends at the centre, where the arc vanishes), and
// every corner where the walls' offsets part a pyramid on the corner whose
// sides lie in the two walls' leaned planes (the mitre). A widening taper
// is the prism with them all (fused: where two walls meet, their wedges
// meet and the gap between them fills from that height on), a narrowing
// one the prism without them (cut: a narrow part both of whose walls
// sweep through it ends where they meet). Faces in one plane or surface
// are merged; the leaned walls carry the names of the prism's walls, the
// caps theirs. Null where a piece or the boolean fails, or the result is
// not a valid solid. Throws std::invalid_argument (taper_vanishes_error)
// when nothing of the profile reaches the prism's far end.
ShapePtr resolved_taper(const Shape& solid, const std::vector<bool>& leaned, int near, const gp_Pln& neutral,
                        const gp_Dir& pull, double angle);

// The message of a taper that closes the whole profile before the end of
// the extrusion (placed_error: the words, the heights for agents, the
// middles of the profile's edges as places). The message alone is
// KernelError::TAPER_CLOSES_PROFILE in core/model-base, which lists it
// for the translations.
std::string taper_vanishes_error(const Shape& solid, int near, double angle, double height, double reached);

// The message of a taper that closes part of the profile on its way where
// that could not be built (mitcad#1854): the words, KernelError::
// TAPER_CLOSES_PART in core/model-base, and `detail` for agents.
std::string taper_closes_part(const std::string& detail, const std::vector<ErrorPlace>& places);

} // namespace mitcad::geometry::detail
