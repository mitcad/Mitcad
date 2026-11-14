// SPDX-License-Identifier: MIT
#pragma once

// Surface modelling, surface sweeps (mitcad#455): surface bodies swept from
// chains of sketch curves (straight, about an axis, along a path, through
// sections) and a sweep's walls without its caps. None changes its inputs.
// Lengths are millimetres, angles radians.

#include <string>
#include <vector>

#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The shape's faces but those of `feature` in one of the `roles` (a face
// named "<feature>:<role>", with a key or split pieces), as a surface body:
// one shell per shell of the shape (a compound of several), faces oriented
// as they were. The faces keep their names. Throws when no face is left.
ShapePtr open_faces(const Shape& shape, const std::string& feature, const std::vector<std::string>& roles);

// The chains (each one or more curves end to end in order, open or closed)
// moved by `shift` and swept straight from `-two` to `one` along
// `direction` (one + two > 0). Faces "<feature>:side(<curve>)".
ShapePtr extrude_sheet(const std::string& feature, const std::vector<Path>& chains, const gp_Vec& shift,
                       const gp_Dir& direction, double one, double two);

// The chains turned about `axis` by `angle1` (right hand about the axis
// direction) and `angle2` the other way; at most a full turn together.
// Faces "<feature>:side(<curve>)".
ShapePtr revolve_sheet(const std::string& feature, const std::vector<Path>& chains, const gp_Ax1& axis,
                       double angle1, double angle2);

// The chains swept along the path from where it crosses `plane` (the
// sketch's): `extent1` of the path after that point and `extent2` of the
// path before it (fractions; of the whole length on a closed path). The
// perpendicular orientation keeps the chains' angle to the path, the
// parallel one their direction. Faces "<feature>:side(<curve>)" (#k pieces
// along the path's curves).
ShapePtr sweep_sheet(const std::string& feature, const std::vector<Path>& chains, const Path& path,
                     const gp_Pln& plane, double extent1, double extent2, SweepOrientation orientation);

// A surface through the sections in order (each one chain; all open or all
// closed), smooth or `ruled`; `closed` runs on from the last section back
// to the first. Faces "<feature>:side(<curve>)" after the first section's
// curves.
ShapePtr loft_sheet(const std::string& feature, const std::vector<Path>& sections, bool ruled, bool closed);

// The chains (each open or closed) moved by the screw motion about `axis`
// that turns `revolutions` times right-handed about the axis direction
// (left-handed with `left_handed`) while it rises `pitch` per turn along
// it (mitcad#1777: a line gives a helicoid). Faces
// "<feature>:side(<curve>)".
ShapePtr helix_sheet(const std::string& feature, const std::vector<Path>& chains, const gp_Ax1& axis, double pitch,
                     double revolutions, bool left_handed);

} // namespace mitcad::geometry
