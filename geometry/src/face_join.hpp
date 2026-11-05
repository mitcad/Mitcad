// SPDX-License-Identifier: MIT
#pragma once

// Faces that are pieces of one surface joined into one face, and the edges
// that then run along one curve between the same two faces joined into one
// edge (mitcad#343): a thread's flanks, root and crest, cut a turn at a
// time, made one face each over all the turns, as the bodies stored in
// files keep threads. Unlike OCCT's face merge (ShapeUpgrade_UnifySameDomain)
// a face may run many periods of a periodic surface (a crest about a
// cylinder): its p-curves are unrolled over the periods. Internal to the
// geometry library.

#include <string>
#include <vector>

#include <Geom_Surface.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// `body` with those of the faces `faces` (indices) that share their surface
// (the same surface or copies of one, in the same location) and are
// connected through their edges each joined into one face holding all
// their names, and then each run of
// two or more edges of the joined faces that meet tangentially at vertices
// of no other edge, between the same two faces, joined into one edge. The
// other faces keep their geometry and names. Null where the result would
// not be valid; the shape unchanged where nothing is to be joined.
ShapePtr join_surface_pieces(const Shape& body, const std::vector<int>& faces);

// The result of an operation on join_surface_pieces' body (of `body` with
// its faces `faces` joined) with its faces split again where the pieces
// met, so that it keeps the pieces as `body` did: the faces over the joined
// pieces and the faces the operation generated along their edges (named
// face_name(feature, role, <edge>)) split by the planes of the edges
// between the pieces, over them and `reach` round them. A piece of a face
// over the joined pieces is named as the piece of `body` it lies on, a
// piece of a generated face as the face generated from the piece's edge it
// runs along; the pieces of any other face it splits are joined again.
// Null where the result would not be valid.
ShapePtr split_joined_pieces(const Shape& result, const Shape& body, const std::vector<int>& faces,
                             const std::string& feature, const std::string& role, double reach);

// Whether two surfaces are one: the same, or copies of one (a cylinder of
// the same frame and radius, a B-spline of the same poles and knots).
bool same_surface(const occ::handle<Geom_Surface>& a, const occ::handle<Geom_Surface>& b);

// How far apart two faces' surfaces may lie and still be one surface
// (join_same_surfaces): their parameters within 1e-6 mm, their axes and
// normals within 1e-9 rad.
inline constexpr double kOneSurface = 1.0e-6;
inline constexpr double kOneSurfaceAngle = 1.0e-9;

// `body` with the faces that lie on one plane, cylinder, cone, sphere or
// torus (within kOneSurface and kOneSurfaceAngle, whatever their surfaces'
// frames: coplanar planes, coaxial cylinders of one radius, ...), facing
// the same way, and meet along an edge joined into one face that holds all
// their names, the edges between them gone (mitcad#1261): a cut's wall
// from the arcs of one sketch circle, its pieces' surfaces a few 1e-7 mm
// apart as a solved sketch leaves them, which OCCT's merge after a boolean
// (exact without a fuzzy value) leaves apart, and the rim it goes on from.
// OCCT's face merge (ShapeUpgrade_UnifySameDomain) joins them, on a copy
// of the body and only across the edges between such faces, with that
// tolerance, each piece first put on its group's first piece's surface. A
// face closed round its axis whose seam is in pieces (pieces joined with
// their seams at other angles) is made anew with one seam. The faces of a
// group whose joined faces would not be valid stay apart, each such group
// named in an info of the result (its reason); a group that would make a
// face winding round its axis more than once (a thread's turns) stays
// apart. `body` itself where nothing changes. B-spline pieces (a thread's
// flanks) are join_surface_pieces' and are left as they are.
ShapePtr join_same_surfaces(const ShapePtr& body);

} // namespace mitcad::geometry::detail
