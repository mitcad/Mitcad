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

} // namespace mitcad::geometry::detail
