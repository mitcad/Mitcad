// SPDX-License-Identifier: MIT
#pragma once

// Roundings that run over cut-outs or protrusions of their faces
// (mitcad#311).
//
// A rounding's contact on one of its faces may cross a region where the
// face's surface is interrupted: a cut-out (a flat, slot or hole cut into
// the face after it was made: material missing on the face's material
// side) or a protrusion (a lug or rib standing out of the face: material on
// its outer side). OCCT's rolling ball fails there (it rolls on the
// region's sharp rim), where the bodies stored in .f3d files keep the
// rounding on the face's own surface across the region and trim it by the
// region's faces, which grow out to meet it. That is the rounding of the
// body with the cut-outs filled and the protrusions taken away, back to the
// face's surface, with the regions put back afterwards: each as the region
// on the region's side of every face of the body that bounds it (its
// walls), near it; a cut-out cut out again, a protrusion joined again with
// the body's material there.

#include <array>
#include <functional>
#include <optional>
#include <string>
#include <vector>

#include <TopoDS_Shape.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// A body with the cut-outs and protrusions of its selected edges' faces
// cleared.
struct Cutouts {
  // The cleared body, its faces named after the input's.
  ShapePtr body;
  // The sets of the cleared body's edges (as `owner` of the input's).
  std::vector<int> owner;
  // The regions cut out again (one solid per cut-out) and the body's
  // material joined again (one solid per protrusion).
  TopoDS_Shape recut;
  TopoDS_Shape rejoin;
  // The input's faces whose surfaces the cleared body and the result merge
  // faces on: the selected edges' faces and the regions' walls.
  std::vector<int> faces;
  // A face of the result on none of their surfaces (a wall continued past
  // the face it stands on, its surface extrapolated, fill_overflow) takes
  // the name of the nearest of them (not of `spine`, the selected edges'
  // faces).
  bool nearest_names = false;
  std::vector<int> spine;
  // The walls fill_overflow took away (rising past a narrow face's far
  // edge), which put_walls_back puts back into `rejoin`.
  struct Wall {
    TopoDS_Shape solid;     // the wall's material taken away
    TopoDS_Shape continued; // with the body's material under it, its faces continued down
    int face = -1;          // the narrow face it rises from
    std::array<double, 6> within{}; // the box it was looked for in
  };
  std::vector<Wall> rising;
};

// The body with the cut-outs filled and the protrusions taken away that a
// rounding of the selected edges (`owner`, by edge: the set index,
// negative for none) runs over: `contact` is the largest distance from an
// edge at which the rounding touches its faces, `reach` the largest from
// an edge at which it changes the body. A cut-out is a region outside the
// body on the material side of the surface of one of the edges' faces, a
// protrusion a region of the body on its outer side, within `reach` of the
// edges, that meets the face along one of its other edges, lies on that
// surface and comes within `contact` of the edges. Each is taken only
// where the region on its side of every face of the body that bounds it
// (planes, cylinders, cones, spheres and tori) holds all of it and, for a
// cut-out, none of the body, for a protrusion, no other part of the body
// past the face's surface. None when no region is found.
std::optional<Cutouts> fill_cutouts(const Shape& body, const std::vector<int>& owner, double contact,
                                    double reach);

// Chamfers wider than a narrow face beside their edges (mitcad#1232): the
// body with the narrow faces continued past their far edges, so that the
// chamfer's contact stays on them. Next to each face of the selected edges
// (`owner`), within `contact` (the largest distance the chamfer reaches
// along a face) of the edges: a region outside the body on the face's
// material side that meets the face along one of its edges other than the
// selected ones and those meeting them, and does not touch the selected
// edges (the space past the narrow face's far edge, under its surface:
// filled, `recut` the region itself), and a region of the body on its
// outer side meeting it so (a wall rising past the far edge, mitcad#1238 on
// any of these faces: taken away, `rejoin` the body's material in the
// region continued into the material under the face's surface, the wall's
// faces grown down to the chamfer along their own surfaces). Near an end of a chain of the selected edges the regions
// are looked for only on the edges' side of the faces at that end, so that
// they do not run round the chain's end. The dressup of `body` (the
// chamfer at its full size) less the regions filled, with the walls put
// back, is the chamfer cut by the faces beyond the narrow faces, which
// vanish where the chamfer is wider. Any surface may lie beyond; the face
// itself is a plane, cylinder, cone, sphere or torus. Walls only with
// `walls_too` (else the geometry kernel cuts the chamfer by them where it
// can); `steepness` is the largest ratio of the chamfer's two distances,
// for how deep it can go under a narrow face past its far edge. With
// `at_corners` (mitcad#1238, where an overflowing chamfer meets others at a
// vertex) the regions are looked for for each selected edge of a face on
// its own, past the face's edges that do not meet that edge, on the edge's
// side of every other face at its ends; they may touch the other selected
// edges' ends, and edges running on there past a region are merged. None
// when no region is found.
std::optional<Cutouts> fill_overflow(const Shape& body, const std::vector<int>& owner, double contact,
                                     double steepness, bool walls_too, bool at_corners = false);

// Where a chamfer of the selected edges (`owner`) reaching `contact` along
// its faces is wider than a face beside an edge (a far edge of the face,
// not at the selected edges' vertices, nearer the edge than `contact`):
// "the chamfer of edge E{…} is wider than face F… and cannot meet face F…
// beyond it" for the first such edge and face (with how many more), for an
// error when the chamfer cannot be built so. Empty when there is none.
std::string overflow_failure(const Shape& body, const std::vector<int>& owner, double contact);

// Where a chamfer of the selected edges (`owner`) reaches exactly to a far
// edge of a face beside them, its contact on the face touching that edge
// at a tangent (mitcad#1260: the face would end in a point there, between
// the parts the chamfer runs over and those it leaves): "the chamfer
// reaches exactly to the far edge of the narrow face beside its edge: …",
// in plain words (mitcad#1613), for an error where the chamfer built so is
// not sound. `reach(e, f)` is how far
// the chamfer of selected edge `e` reaches along its face `f` (the faces'
// far edges are those overflow_failure looks at). Empty when there is none.
std::string overflow_tangent(const Shape& body, const std::vector<int>& owner,
                             const std::function<double(int, int)>& reach);

// Puts the walls fill_overflow took away (`cutouts.rising`) into
// `cutouts.rejoin`, given the chamfer of the cleared body (`chamfered`,
// its new faces named `generated` and more): where the chamfer's surface
// comes out of a wall again near the narrow face's far edge, the wall cuts
// the chamfer there (the wall less its part on the chamfer's open side
// next to the far edge); where it does not, the chamfer is cut at the far
// edge, the wall continued down to it. False when that fails.
bool put_walls_back(Cutouts& cutouts, const Shape& chamfered, const std::string& generated, const Shape& input);

// The dressup of the cleared body (`rounded`) with the regions put back,
// its faces named after the dressup's and, where a region's face is new,
// after the input's face it lies on; the pieces of one face of the input,
// or of one selected edge's face, on one surface merged. Null when that is
// not one valid solid; then, where only the merge made it invalid,
// `unmerged` (when given and still null) gets the result with those pieces
// apart (mitcad#1613), for the caller to take when nothing else builds.
ShapePtr cut_out_again(const ShapePtr& rounded, const Cutouts& cutouts, const Shape& input,
                       ShapePtr* unmerged = nullptr);

} // namespace mitcad::geometry::detail
