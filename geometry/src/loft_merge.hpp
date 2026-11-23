// SPDX-License-Identifier: MIT
#pragma once

// Tangent Edges: Merge (mitcad#2162). A loft's section whose edges meet
// tangentially is joined into smooth edges before the loft: each run of
// edges between sharp corners becomes one edge, a section smooth all round
// one closed edge. The joined curve runs at one speed (by arc length, a
// polynomial B-spline within 1e-8 mm of the edges), so its first
// derivative is the same on both sides of every former join: the loft's
// cross tangent, which blends the sections' derivatives, then turns
// smoothly there and the side is tangent-continuous (G1). Along each
// former edge it is continuous in its second derivative too, so that only
// the former joins are lines where the side's curvature may jump
// (mitcad#2271), and a ring is continuous in its first derivative where it
// closes, so that the side closes smoothly round it. Sections smooth
// all round start at corresponding points (each where the ray from its
// centre towards the neighbouring section's start meets it, as OCCT's
// matching of closed sections relates their vertices) and run the same way
// round. Internal to the geometry library.

#include <string>
#include <utility>
#include <vector>

#include <TopoDS_Edge.hxx>

#include "spine.hpp"

namespace mitcad::geometry::detail {

// Two edges meet tangentially when their tangents there lie within this
// angle (radians): the model's angular tolerance.
constexpr double kTangentJoinAngle = 1.0e-6;

// A joined curve lies within this distance (mm) of the edges it joins.
constexpr double kJoinTolerance = 1.0e-8;

// A loft section to join: its wire and the names of the faces its edges
// make (nothing for a point). merge_tangent_edges replaces `named` by the
// joined wire, each new edge named after the first edge of its run (a
// section smooth all round after its edge whose name sorts first), and
// lists in `sources` each new edge with the section's edges it joins.
struct MergeSection {
  bool point = false;
  NamedWire named;
  std::vector<std::pair<TopoDS_Edge, std::vector<TopoDS_Edge>>> sources;
  // One closed edge, smooth all round.
  bool smooth = false;
};

// Joins the runs of tangent edges of every section that is not a point.
// A section without such a run is left as it is (its wire unchanged). True
// when every section that is not a point became one smooth closed edge:
// they then start at corresponding points and run the same way round, and
// loft edge for edge without OCCT's matching of the sections.
bool merge_tangent_edges(std::vector<MergeSection>& sections);

// The section's edges a lofted edge came from: the joined ones for a new
// edge, else the edge itself.
std::vector<TopoDS_Edge> source_edges(const std::vector<std::pair<TopoDS_Edge, std::vector<TopoDS_Edge>>>& sources,
                                      const TopoDS_Edge& edge);

} // namespace mitcad::geometry::detail
