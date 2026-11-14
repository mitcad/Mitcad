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
// Only reads the shape; the model keeps every edge (names, queries,
// selections and features that name them are unchanged).

#include <string>

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

// Whether the faces at the edge meet smoothly across it (first == second
// for a seam): its stored continuity between them when one is coded,
// else OCCT's (BRepLib::ContinuityOfFaces) at least G1 within
// kSmoothSeamAngle. False when it cannot be worked out.
bool smooth_across(const TopoDS_Edge& edge, const TopoDS_Face& first, const TopoDS_Face& second);

// The edges of `shape` that lie inside one smooth surface: each between a
// face and itself (a seam), or between two faces on one surface (the same
// surface of OCCT's, or, with `named`, two pieces of one named face: names
// equal but for their "#k" piece numbers), where smooth_across holds.
// `named`, when given, is the Shape whose occt() `shape` is (its face
// names); null for a shape of the view's own (a highlight's faces), which
// is judged by its seams and surfaces only. Free edges and edges of three
// or more faces are never in it.
ShapeMap smooth_seams(const TopoDS_Shape& shape, const Shape* named = nullptr);

// A face name without its trailing "#k" piece numbers (`F5:fillet(..)#0#1`
// -> `F5:fillet(..)`): what the pieces of one split face share.
std::string unsplit_face_name(const std::string& name);

} // namespace mitcad::geometry
