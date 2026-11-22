// SPDX-License-Identifier: MIT
#pragma once

// Edges that lie inside one smooth surface (mitcad#1773): the views leave
// them out. A closed cylinder, cone, sphere or torus face has a seam edge
// where its angle wraps from a whole turn back to 0, with the same face on
// both sides; a face that an operation split into pieces on one surface
// (named `<face>#0`, `<face>#1`, naming.hpp) has edges between the
// pieces. Where the surface runs on across such an edge without a change
// of direction (its normals on both sides agree along the edge within
// kSmoothSeamAngle), nothing is there to see: the 3D view and its hover do
// not draw it. Where it creases (a closed profile with a sharp corner
// extruded as one face, its seam at the corner), the edge is drawn as
// before; so is every edge between two different faces, also where they
// meet tangentially (a fillet and the faces it rounds).
//
// Pieces of one named face are not always on one surface (mitcad#2148): a
// circle swept along a line and a tangent arc gives one side face in two
// pieces, `side(..)#0` a cylinder and `side(..)#1` a torus, which meet
// tangentially. The edge between two pieces is drawn where both are
// analytic surfaces (plane, cylinder, cone, sphere, torus) that are
// provably different (different_analytic_surfaces); every other edge
// between pieces (B-splines and other free-form pieces, such as a fillet
// split by a ball's seam) keeps the rule above.
//
// Only reads the shape; the model keeps every edge (names, queries,
// selections and features that name them are unchanged).

#include <cstdint>
#include <string>
#include <vector>

#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The largest angle between the surface's normals on the two sides of a
// seam, in radians, at which it is taken as smooth: the angular tolerance
// of the tangent chains (face_select.hpp, kTangentChainAngle) and of OCCT's
// fillets, about 0.57 degrees, a kink a view does not show.
constexpr double kSmoothSeamAngle = 1.0e-2;

// How far apart two analytic surfaces' defining values may be and still
// count as one surface (mitcad#2148): lengths (a distance, a radius) in
// millimetres, angles (between axes or normals, of a cone's half-angle) in
// radians, a micrometre over 100 mm. Pieces of one face that were stored
// or built apart (another reference frame, rounded radii) differ by far
// less; surfaces that are really different (a cylinder and the torus it
// runs into, two tori about different centres) by far more.
constexpr double kOneSurfaceLength = 1.0e-3;
constexpr double kOneSurfaceAngle = 1.0e-5;

// Whether the faces at the edge meet smoothly across it (first == second
// for a seam): its stored continuity between them when one is coded,
// else OCCT's (BRepLib::ContinuityOfFaces) at least G1 within
// kSmoothSeamAngle. False when it cannot be worked out.
bool smooth_across(const TopoDS_Edge& edge, const TopoDS_Face& first, const TopoDS_Face& second);

// Whether the two faces lie on analytic surfaces (plane, cylinder, cone,
// sphere, torus) that are provably different (mitcad#2148): of different
// types, or of one type whose defining values, with each face's placement
// applied and whatever the surface's own reference frame, differ by more
// than kOneSurfaceLength or kOneSurfaceAngle: a plane by the plane itself
// (a point's distance and the normal, either way), a cylinder by its axis
// as a line (either way, any origin on it) and radius, a cone by its apex,
// axis line and half-angle, a sphere by its centre and radius, a torus by
// its axis line, centre and both radii. False for any other surface (a
// B-spline, an offset or swept surface), and when it cannot be worked out.
bool different_analytic_surfaces(const TopoDS_Face& first, const TopoDS_Face& second);

// The edges of `shape` that lie inside one smooth surface: each between a
// face and itself (a seam), or between two faces on one surface (the same
// surface of OCCT's, or, with `named`, two pieces of one named face: names
// equal but for their "#k" piece numbers, unless
// different_analytic_surfaces), where smooth_across holds.
// `named`, when given, is the Shape whose occt() `shape` is (its face
// names); null for a shape of the view's own (a highlight's faces), which
// is judged by its seams and surfaces only. Free edges and edges of three
// or more faces are never in it.
ShapeMap smooth_seams(const TopoDS_Shape& shape, const Shape* named = nullptr);

// A surface number for each face of `shape`, by face index (mitcad#2148,
// for model_render's raster): faces joined by an edge of
// smooth_seams(shape.occt(), &shape) between two different faces share
// one, every other face has its own; numbered from 0 in the order of the
// faces' first appearance.
std::vector<std::uint32_t> smooth_surface_groups(const Shape& shape);

// A face name without its "#k" piece numbers, wherever they stand
// (`F5:fillet(..)#0#1` -> `F5:fillet(..)`, and the pieces of a patterned
// or mirrored copy `F4:inst1(F2:side(c6)#1)` -> `F4:inst1(F2:side(c6))`):
// what the pieces of one split face share.
std::string unsplit_face_name(const std::string& name);

} // namespace mitcad::geometry
