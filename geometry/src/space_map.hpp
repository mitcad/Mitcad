// SPDX-License-Identifier: MIT
#pragma once

// A solid carried through a smooth map of space (mitcad#1854): a taper
// built straight and flat, where its topology events are resolved
// (taper_events.hpp), then bent onto a cylinder or a cone (a wrapped
// Emboss) or along a curved path (a sweep). Internal to the geometry
// library.

#include <functional>
#include <optional>

#include <Geom_Surface.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// A smooth map of space, one to one over the shape it carries (and a
// little around it: its derivatives are taken by differences).
using SpaceMap = std::function<gp_Pnt(const gp_Pnt&)>;

// A planar face of the shape laid onto a surface that the map takes its
// plane onto exactly (a cap that must lie on a body's face, not within a
// tolerance of it): the surface, and its parameters of each point of the
// face's plane; empty where they are the plane's own (the map moves the
// plane rigidly and `surface` is the plane so moved).
struct LaidFace {
  occ::handle<Geom_Surface> surface;
  std::function<gp_Pnt2d(const gp_Pnt&)> parameters;
};

// The surface a face of the shape is laid onto, or none (the face's image
// is approximated).
using LaidFaces = std::function<std::optional<LaidFace>(const TopoDS_Face&)>;

// The shape carried through the map, face for face: each face's surface
// replaced by a B-spline within `tolerance` of the map of it, over the
// face's parameters (so that its curves on the surface stay as they are),
// or by the surface `laid` gives it, with its curves on the surface mapped
// to that surface's parameters; each edge's curve by a B-spline within
// `tolerance` of the map of it, with the same parameters; each vertex
// mapped. A map that turns space inside out turns the faces over. The
// faces keep their names. Null where an approximation fails, or the result
// is not a valid solid.
ShapePtr mapped_solid(const Shape& shape, const SpaceMap& map, const LaidFaces& laid, double tolerance);

} // namespace mitcad::geometry::detail
