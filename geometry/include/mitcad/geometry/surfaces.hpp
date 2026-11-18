// SPDX-License-Identifier: MIT
#pragma once

// Patch, Stitch and Thicken (mitcad#456): surface bodies that fill sketch
// regions or a closed loop of a body's edges, bodies' faces sewn into one
// body, and surface bodies made solid by a wall. None changes its inputs.
// Lengths are millimetres.

#include <string>
#include <utility>
#include <vector>

#include <gp_Pnt.hxx>

#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/surface_offsets.hpp"

namespace mitcad::geometry {

// One surface body per region, in order: a shell of the region's planar
// face, "<feature>:patch(<region name>)", its normal the frame's.
std::vector<ShapePtr> patch_regions(const std::string& feature, const Frame& frame,
                                    const std::vector<Region>& regions);

// A loop of free edges of a body: edges that bound one face only (not a
// seam), connected through their vertices. The open boundaries of a
// surface body.
struct OpenBoundary {
  // The primary names of the faces its edges bound, sorted, each once;
  // unnamed faces are left out.
  NameList faces;
  // Points along its edges, their ends included, at most about 0.25 mm
  // apart (up to 4000 per edge).
  std::vector<gp_Pnt> points;
  // Whether the edges close up: every vertex ends two of them.
  bool closed = false;
};

// The body's open boundaries in a deterministic geometric order.
std::vector<OpenBoundary> open_boundaries(const Shape& body);

// The free edges of each open boundary (edge indices of `body`) and the
// faces they bound, as OpenBoundary::faces, in the order of
// open_boundaries, without its points (mitcad#1927: the view picks a free
// edge as the opening it lies on, which a definition names by its faces
// and its index among the openings along the same faces).
struct OpenBoundaryEdges {
  NameList faces;
  std::vector<int> edges;
};
std::vector<OpenBoundaryEdges> open_boundary_edges(const Shape& body);

// How a patch meets the face beside an edge of its loop: through the edge,
// tangent to the face, or tangent and of the same curvature.
enum class Continuity { G0, G1, G2 };

// The loop a patch fills: edge references of the body (every edge each
// resolves to) and free edges of its openings (mitcad#1955: single free
// edges and named edges in one loop, which can close part of an opening),
// or the open boundary of the body at `opening` (an index of
// open_boundaries) when it is not negative. Beside the edges, `curves`
// (mitcad#1972) are sketch curves in model space, in no order, whose ends
// meet the edges' ends or each other within 0.01 mm: with free
// edges, a line across an opening closes part of it. The patch only passes
// through them (G0).
struct PatchLoop {
  std::vector<std::string> edges;
  std::vector<OpeningEdges> openings;
  Path curves;
  int opening = -1;
  Continuity continuity = Continuity::G0;
  // Edges of the loop that meet their face otherwise.
  std::vector<std::pair<std::string, Continuity>> edge_continuity;
  // Points the patch passes through.
  std::vector<gp_Pnt> points;
};

// A surface body of one face, "<feature>:patch", filling the closed loop:
// a plane when the loop lies in one and the patch only passes through its
// edges (G0 everywhere, no points), else a surface fitted to the edges
// that meets the face beside each edge with its continuity and passes
// through the points. G1 and G2 need the one face beside the edge: an edge
// between two faces is only passed through. Throws when the edges do not
// close up into one loop.
ShapePtr patch_loop(const std::string& feature, const Shape& body, const PatchLoop& loop);

// The shapes' faces sewn together where their edges lie within
// `tolerance` (0: 1e-6 mm): a closed shell becomes a solid, an open one
// stays a surface. One shape (a compound of several parts when they do not
// all connect). The faces keep their names.
ShapePtr stitch(const std::vector<const Shape*>& shapes, double tolerance);

// The surface body (faces or open shells, no solid) made solid by a wall
// `thickness` thick along its faces' normals (the faces' orientations
// applied), against them when negative, or half of it to each side when
// `symmetric`. The wall's faces are named after `feature`:
//   <feature>:front(<face>)  the side the face's normal points to
//   <feature>:back(<face>)   the other side
//   <feature>:wall(<face>)   along the face's free edges ("#k" when several)
// Throws when the wall does not close up into a solid.
ShapePtr thicken(const std::string& feature, const Shape& sheet, double thickness, bool symmetric);

} // namespace mitcad::geometry
