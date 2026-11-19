// SPDX-License-Identifier: MIT
#pragma once

// Fillets and chamfers with Tangent Chain off whose geometry kernel chain
// runs on past the selected edges (mitcad#2037).
//
// OCCT's fillets and chamfers always extend a selected edge along the edges
// that continue it smoothly (a straight edge running into a rounded corner),
// and cannot end a rounding or bevel at such a vertex. With Tangent Chain
// off, only the selected edges are to be dressed. The dressup is built over
// the kernel's whole chain (`full`), and the body is kept as it was over the
// chain's edges no set selects: inside a bounded tube round each of them
// (lofted circles in the planes normal to the edge, ending in those planes
// at its vertices and, at an open chain's ends, twice its radius past them), the
// result is the body's material, outside it the full dressup's. Where a
// selected edge meets an unselected one, the tube round that next edge
// leaves out a tapered region, so that the rounding or bevel runs out over
// it: in each section normal to the edge, the dressup's cross-section
// (the bevel's line, the rounding's arc) is moved towards the edge by a
// depth that grows linearly with the length along the edge, from a little
// below the full section at the shared vertex to the edge itself just
// before the edge's far vertex (2 % of its length), where the run-out ends
// in a point. Without the run-out (the next edge is shorter than the
// dressup is wide, or the taper does not build), the tube ends flat in the
// plane normal to the edge at the shared vertex, and the dressup ends there
// with a flat face.
//
// The result is made by OCCT's booleans; the input shapes are not changed.
// The faces keep the dressup's names; the run-out's face is
// <feature>:runout(<edge>) (the edge it runs out over), the flat end's
// <feature>:runout_end(<vertex>) (the vertex it ends at).

#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// The size of a set's rounding or bevel across an edge.
struct RunOutSize {
  bool fillet = false;
  double radius = 0.0;    // a fillet's constant radius; 0 with a chord
  double chord = 0.0;     // a fillet's width across the rounding
  double distance = 0.0;  // a chamfer's distance on the measured face
  double distance2 = 0.0; // two distances: on the other face (0: equal)
  double angle = 0.0;     // distance-angle: radians from the measured face
  int measured = -1;      // the body's measured face (two distances, angle)
};

// A chain (contour) of the geometry kernel that holds selected edges of one
// set without a tangent chain and runs on into edges no set selects.
struct PartialChain {
  std::vector<int> edges;     // the body's edges, in order along the chain
  std::vector<char> selected; // per edge: selected by the set
  bool closed = false;
  int set = -1;
  RunOutSize size;
};

// The geometry kernel's chains of the selected edges (`owner`: each edge's
// set, -1 for none) that run past the edges of a set with `chain_off` set,
// into edges no set selects; chains that hold edges of several sets are
// left out. `chamfer` takes the chamfer's chains, else the fillet's.
std::vector<PartialChain> partial_chains(const Shape& body, const std::vector<int>& owner,
                                         const std::vector<char>& chain_off, bool chamfer);

// The body with the dressup `full` (built over each chain whole) kept over
// the chains' selected edges only, running out over the next edge where a
// selected edge continues smoothly into an unselected one, or with
// `run_out` false ending flat there. Null when it cannot be built: the
// tubes or tapers fail, or the result is not one valid solid that shows
// none of their other faces.
ShapePtr over_selected_edges(const std::string& feature, const Shape& body, const Shape& full,
                             const std::vector<PartialChain>& chains, bool run_out);

} // namespace mitcad::geometry::detail
