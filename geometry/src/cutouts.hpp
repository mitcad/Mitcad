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
#include <utility>
#include <vector>

#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>

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
    // Put back whole (`continued`), never cut by the dressup: a wall found
    // within its face's sector (fill_overflow's `sectors`, mitcad#1849).
    bool whole = false;
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
// faces grown down to the chamfer along their own surfaces). A far edge
// that meets a selected edge where the face ends in a point between them
// (a circular segment beside a chord whose ends lie on its arc,
// mitcad#1783) counts too, its regions touching the selected edges at those
// points only; for concave edges under a planar face only a slab a quarter
// of `contact` deep is filled. Near an end of a chain of the selected edges the regions
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
// How far a dressup reaches along a face of an edge: (body, the edge's set,
// the edge, the face), each by index in `body` (mitcad#1819).
using OverflowReach = std::function<double(const Shape&, int, int, int)>;

// With `reach`, a face counts as narrow only where its far edge is nearer
// its edges than the dressup reaches along that face (a two-distance
// chamfer's short leg on the face it lies on) rather than `contact`
// (mitcad#1819: a face narrower than the long leg only is no narrow face);
// the regions are looked for within `contact` as before.
//
// With `sectors` (mitcad#1849) each face's regions are looked for only in
// its own part of the chain: on the face's side of a plane through each of
// its edges that runs from a corner of the chain (a vertex where the face's
// selected edge meets the next selected edge), the plane parting the two
// edges' directions there (at a tangent corner the plane across the chain,
// at a sharp one the mitre). A face round a corner of a rounded chain (a
// cylinder segment) then finds the step past it within its own sector,
// where its surface, continued, would take in the next faces' material too,
// and the walls are put back whole, the faces beyond grown out to meet the
// dressup (the step past the narrow face of a closed chain lies wholly
// within the regions' reach, where put_walls_back would else cut it by the
// dressup). A face without such a plane is left out; none when no face has
// one.
std::optional<Cutouts> fill_overflow(const Shape& body, const std::vector<int>& owner, double contact,
                                     double steepness, bool walls_too, bool at_corners = false,
                                     const OverflowReach* reach = nullptr, bool sectors = false);

// Where a chamfer (or a fillet: `role`) of the selected edges (`owner`)
// reaching `contact` along its faces is wider than a face beside an edge (a
// far edge of the face, not at the selected edges' vertices, nearer the
// edge than `contact`): the face it would take away and the faces beyond
// it, which it cannot meet, as placed_error() gives it: in words for the
// person ("the chamfer is wider than the face beside its edge and would
// take that face away, but cannot meet the face beyond it"), the edge and
// faces as its places, "the chamfer of edge E{…} is wider than face F…,
// which it would take away, and cannot meet face F… beyond it" for agents,
// for the first such edge and face (with how many more), for an error when
// the dressup cannot be built so (mitcad#1232, mitcad#1819). Empty when
// there is none. `cause`, where given, follows in both texts: why the faces
// beyond cannot meet it (kFreeformCause, mitcad#1849).
std::string overflow_failure(const Shape& body, const std::vector<int>& owner, double contact,
                             const char* role = "chamfer", const OverflowReach* reach = nullptr,
                             const char* cause = nullptr);

// overflow_failure's cause where a face beside the edges is a free-form
// B-spline surface (no plane, cylinder, cone, sphere or torus within 1e-4),
// which the dressup past narrow faces cannot continue past its far edge
// (mitcad#1849). In words, no names.
inline constexpr const char* kFreeformCause =
    ": the face beside its edge is a free-form surface, which cannot be continued past its far edge";

// Where a chamfer of the selected edges (`owner`) reaches exactly to a far
// edge of a face beside them, its contact on the face touching that edge
// at a tangent (mitcad#1260: the face would end in a point there, between
// the parts the chamfer runs over and those it leaves): "the chamfer
// reaches exactly to the far edge of the narrow face beside its edge: …",
// in plain words (mitcad#1613), for an error where the chamfer built so is
// not sound; as placed_error() gives it (mitcad#1617), its places the
// chamfered edge, the far edge and the point where the contact touches it,
// its detail naming them, the face and the face beyond, and the point, for
// agents. `reach(e, f)` is how far
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

// A rounding of a concave edge wider than a narrow face beside it whose far
// edge goes on tangentially into a convex face beyond (a step's riser that
// runs into a round face, mitcad#2208): the ball takes the narrow face away
// and rolls on over the face beyond, touching it and the edge's other face.
// Continuing the narrow face past its far edge (fill_overflow) would keep
// the ball on the narrow face's surface instead, and the space filled
// between that surface and the face beyond, cut out again, would leave the
// rounding hanging beside the face beyond as a sheet. The body here has the
// narrow face replaced by the face beyond continued along its own surface
// until it meets the edge's other face (the material between the narrow
// face, the face beyond continued and the other face's surface taken away),
// so that the rounding of the new edge between the other face and the face
// beyond, at the same size, is that rounding; the material taken away lies
// inside what the rounding adds where it takes the whole narrow face away
// (the caller checks that its result holds the whole input). Where the face
// beyond bends away from the material instead (a hole's round wall, the
// narrow face a short straight stretch tangent to it before a corner,
// mitcad#2419), the face beyond continued runs out of the material under
// the narrow face: the space between the narrow face, the face beyond
// continued and the other face's surface (between the walls at the strip's
// ends) is filled instead, and it too lies inside what the rounding adds.
struct FacesBeyondGrown {
  // The body with the narrow faces replaced, its faces named after the
  // input's.
  ShapePtr body;
  // The sets of its edges (as `owner` of the input's): the new edges
  // between the other faces and the faces beyond in place of the picked
  // edges beside the narrow faces.
  std::vector<int> owner;
  // The names of the new edges and of their vertices, each with the name
  // of the input's edge or vertex it stands for.
  std::vector<std::pair<std::string, std::string>> renames;
  // The picked edges beside the narrow faces replaced, each with its other
  // face (the face the ball touches beside the narrow one), as edge and
  // face indices of the input (mitcad#2330: added_past_walls).
  std::vector<std::pair<int, int>> replaced;
  // Whether the space past every narrow face was filled (its faces beyond
  // bending away from the material, mitcad#2419) and no material taken
  // away.
  bool filled = false;
};

// The body for the rounding of the selected edges (`owner`, by edge: the
// set index, negative for none) reaching `contact` along their faces, with
// each narrow face beside a concave selected edge replaced as above: a face
// of one selected edge whose far edges (its edges that do not meet that
// edge) are all nearer the edge than `contact` and all go on tangentially
// into faces beyond, convex ones (material taken away) or ones bending
// away from the material (space filled, between the walls at the strip's
// ends, mitcad#2419). The edge's faces, the faces beyond and those walls
// are planes, cylinders, cones, spheres or tori. None when there is no such
// face, or the material to take away (the space to fill) is not bounded by
// the narrow face, the face beyond and the other face's surface (and those
// walls) near the edge.
//
// `rising` (mitcad#2352, GitHub #620): instead, a narrow face beside a
// convex selected edge whose far edges across it (those wholly nearer the
// edge than `contact`; the face's other far edges lie past the rounding)
// all go on concavely into one face beyond that rises from it (a ledge
// before a spike's foot): the narrow face is taken away there with the
// material between it, the edge's other face's surface and the face beyond
// continued down along its own surface, so that the rounding of the new
// edge between the other face and the face beyond, at the same size, is
// the rounding tangent to both that the ball rolling past the ledge makes;
// the material taken away lies outside that rounding (the caller checks
// that its result lies within the input). Continued along its own surface
// instead (over_narrow_faces), the ledge's plane held the ball, and the
// face beyond grown down to meet it buried the rounding (a 4 mm rounding
// beside a ledge 1 wide before a face rising at 55 degrees left a sharp
// edge, with no error).
std::optional<FacesBeyondGrown> faces_beyond_grown(const Shape& body, const std::vector<int>& owner,
                                                   double contact, bool rising = false);

// The material that a rounding of a concave picked edge (`edge` of `body`,
// a straight line) adds under the edge's other face (`face`, a plane: the
// face the ball touches) past that face's side walls (mitcad#2330). The
// rounding `made` (of `body`, faces_beyond_grown's or the kernel's) adds
// material under the face, between its surface and the ball; along the
// edge that material ends where the face does, at the faces standing on
// the face's other edges (an end face, a cut face, a rounding an earlier
// fillet made of a corner there), continued down past the face along
// their own surfaces. Where such a wall does not reach across the
// rounding's section (an earlier rounding of the corner took part of the
// end face away, so that the rounding's contact on the face runs off it
// onto that rounding), the kernel ends the rounding on the end face's
// plane alone and leaves a lip standing out past the earlier rounding, a
// step at its foot. That lip is the material `made` holds past the face's
// walls continued: outside the body, outside the prism of the face along
// its outward normal and on the face's side of the edge (the plane
// through the edge along that normal; past it the rounding lies under the
// edge's narrow face, taken away, and the face beyond). The walls must
// stand at right angles to the face where the lip touches them (then the
// prism's sides are the walls continued); elsewhere this throws a
// std::runtime_error: the rounding cannot be ended on such a wall here.
// Returns the lip, its faces on a wall's surface named after that wall,
// null where `made` holds nothing past the walls; none where it cannot be
// told (the face is no plane, the edge no line, a boolean fails).
std::optional<ShapePtr> added_past_walls(const Shape& body, int edge, int face, const Shape& made);

// The region a dressup took away from `body` (`dressed`: body less it)
// continued past a plane it ends on (mitcad#2330): each of that region's
// faces on `plane` swept `length` along `out` (the way past the plane), its
// faces named after the dressed body's faces on whose surfaces they lie (the
// rounding's, the end faces'). Where a dressup of edges at right angles to
// the plane ends on it with the material past the plane taken off and put
// back afterwards (ended_on_end_planes), the material put back less this
// region is what the dressup leaves where its walls run on down through
// that material. Null where the dressup took nothing away there (it only
// adds); none where it cannot be built.
std::optional<ShapePtr> removal_continued(const Shape& body, const Shape& dressed, const gp_Pln& plane,
                                          const gp_Dir& out, double length);

} // namespace mitcad::geometry::detail
