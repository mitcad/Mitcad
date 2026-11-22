// SPDX-License-Identifier: MIT
#pragma once

// Unions of many shapes in groups (mitcad#1273): the shapes whose boxes
// meet, directly or through others, are united group by group, the groups
// beside each other on all cores, and the groups' results, which share
// nothing, are put together without a boolean operation. One union of all
// of a machine's 257 bodies at once took most of a minute and 12.7 GB;
// OCCT's union intersects every pair of the bodies' boxes that meet and
// keeps the whole operation's data until it ends.
//
// And what booleans of shapes that meet only within micrometres share
// (mitcad#1278): the near misses between them, found before a union of
// many, and the warnings that tell a boolean went wrong there.

#include <cstddef>
#include <functional>
#include <utility>
#include <vector>

#include <Message_Alert.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// The largest fuzzy value a boolean that lost material, or whose pieces
// OCCT left unused, tries without a tolerance of its own (mm, mitcad#1273):
// a micrometre, far below an importer's geometric tolerance (0.01 mm),
// above the noise between parts an assembly places on each other
// (1e-4 mm). Also how far near_misses looks.
constexpr double kUnusedReach = 1.0e-3;

// The first fuzzy value such a boolean tries (mm, mitcad#1273): with
// 1e-5 mm a carriage's thread and a mount's stud coaxial with it came out
// of a union as one solid without a warning that missed most of their
// common volume (2 mm3 of some 120); from 1e-4 mm they join.
constexpr double kUnusedStart = 1.0e-4;

// Whether a warning of OCCT's boolean operation tells of pieces of its
// result it left unused (edges of a face it could not rebuild of its
// pieces, faces it could not make a solid of: the result lacks what those
// built) or of sub-shapes of one operand that its tolerances made meet (an
// acquired self-intersection). Such a result is wrong whatever the
// tolerance (mitcad#1273): OCCT's exact union of shapes that meet only
// within micrometres splits their faces with edges left over.
bool leaves_pieces_unused(const occ::handle<Message_Alert>& alert);

// The shapes grouped by their boxes: two shapes whose boxes, each enlarged
// by `gap`, meet are in one group, and so are shapes linked through others
// (the connected components). Each group lists its shapes' indices in
// input order; the groups come in the order of their first shape. One
// group of all when a shape has no box (an empty shape).
std::vector<std::vector<std::size_t>> box_groups(const std::vector<const Shape*>& shapes, double gap);

// The pairs of shapes whose boxes, each enlarged by `gap`, meet, each pair
// once with the lower index first, sorted; none when a shape has no box.
std::vector<std::pair<std::size_t, std::size_t>> box_pairs(const std::vector<const Shape*>& shapes, double gap);

// The `count` items grouped by the links between them (the connected
// components), each group's items in order, the groups in the order of
// their first item.
std::vector<std::vector<std::size_t>> linked_groups(std::size_t count,
                                                    const std::vector<std::pair<std::size_t, std::size_t>>& links);

// For each of the pairs (of `shapes`, box_pairs'), whether a vertex of one
// lies near a face of the other but not on it: farther from it than their
// tolerances together, which an exact boolean takes as touching, and no
// farther than `reach`. Such near misses are where OCCT's exact boolean of
// shapes that meet only within micrometres goes wrong (mitcad#1278): two
// copies of a plate whose side walls lie 1e-4 mm apart, the vertices of a
// carriage's thread micrometres from the bore of a nut inside it. The
// pairs beside each other.
std::vector<char> near_misses(const std::vector<const Shape*>& shapes,
                              const std::vector<std::pair<std::size_t, std::size_t>>& pairs, double reach);

// Runs job(k) for each k below `count` on OCCT's threads, each as an
// operation of its own (util.hpp's run: OCCT's faults there are its
// errors) under the calling thread's cancel request; once all have run,
// the error of the first job that failed, in the order of k, is thrown.
void parallel_jobs(std::size_t count, const std::function<void(std::size_t)>& job);

// The shapes' solids as one compound with their faces' names, for shapes
// that share nothing to merge.
ShapePtr solids_compound(const std::vector<const Shape*>& shapes);

// The union of the shapes in the groups `groups` (box_groups' of them, of
// more than one group): `unite_group` unites the shapes of each group of
// several, the groups beside each other on OCCT's threads (each under the
// calling thread's cancel request), and a group of one is its shape. The
// first group whose union fails (the largest first) fails the whole with
// its error.
ShapePtr grouped_union(const std::vector<const Shape*>& shapes,
                       const std::vector<std::vector<std::size_t>>& groups,
                       const std::function<ShapePtr(const std::vector<const Shape*>&)>& unite_group);

// Points inside one solid of the shape `shape` (its index among the shapes
// given to points_inside), for missing_inputs (mitcad#2053).
struct InsidePoints {
  std::size_t shape = 0;
  std::vector<gp_Pnt> points;
};

// A few points inside each solid of the shapes (mitcad#2053), the solids
// in the shapes' order: from a point in the middle of each of a solid's
// first faces, the line along the face's inward normal to where it leaves
// the solid, and the point halfway along it that OCCT's classifier finds
// inside the solid; the two deepest of them. A solid where none is found
// (it has no face OCCT can find a point in) has none, and passes
// missing_inputs. The shapes beside each other.
std::vector<InsidePoints> points_inside(const std::vector<const Shape*>& shapes);

// The shapes (indices, in order, each once) of which a solid lies outside
// `result` (mitcad#2053): none of its points (points_inside) lies in or on
// a solid of `result`, within `tolerance`. A union holds every point inside
// each of the shapes it joins. The union of a reported hub with 30 blades,
// triangles, rods and a ring kept the hub and the six blades on it and
// dropped the rest without an error: a valid result that holds more than
// the largest of them, which was all the check of its material asked
// (GitHub mitcad/Mitcad#463). Two points of a solid, both outside, are needed (a
// classifier that misjudges one point does not make a union wrong); a point
// outside the box of every solid of the result is outside without a
// classifier.
std::vector<std::size_t> missing_inputs(const TopoDS_Shape& result, const std::vector<InsidePoints>& points,
                                        double tolerance);

// Whether `shape` lies wholly inside `other`, by OCCT's solid classifier
// (mitcad#2053): the points inside its solids (points_inside), its
// vertices, points along each edge and across each face all lie in or on
// a solid of `other` (within 1e-6 mm). Not by a boolean's volume: for a
// reported rod lying inside a body of the same rod with thin fins, OCCT's
// common was empty, its cut the whole rod, and its general fuse lost a
// piece of the rod, while the classifier put every point of the rod
// inside.
bool lies_inside(const Shape& shape, const Shape& other);

// The shapes (indices, in order) each of which lies wholly inside another
// of them that is not itself left out (mitcad#2053): its box within the
// other's (1e-3 mm around), and lies_inside. A union of the others is
// the union of all. The reported
// design's five rods lie each inside a body of the same rod with three
// thin fins, two of which touch at their tips (an edge on four faces):
// OCCT's exact union of such a pair gave nothing of either, and of
// everything they touch, without a warning. A classifier that fails keeps
// the shape in.
std::vector<std::size_t> contained_inputs(const std::vector<const Shape*>& shapes);

} // namespace mitcad::geometry::detail
