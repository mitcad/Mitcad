// SPDX-License-Identifier: MIT
#pragma once

// Continuity Analysis (mitcad#2521): how smoothly a body's faces meet
// across its edges, sampled along each edge between its two faces.
//
// At each sample the edge's parameter is taken on both faces through their
// curves on the surfaces (pcurves), so the two points are where each face
// says the edge is:
//   gap        the distance between the two faces' points (mm): G0, the
//              faces touch;
//   angle      the angle between the faces' normals out of the material
//              (radians): G1, tangent, where it is about 0;
//   curvature  the difference of the faces' normal curvatures across the
//              edge (1/mm, in the direction square to the edge in the first
//              face's tangent plane): G2, curvature continuous, where it is
//              about 0; compared only where the normals agree within the
//              angle's tolerance (elsewhere the edge is G0 at best, and a
//              direction across one face is not across the other). Its
//              share is the difference over the larger of the two
//              curvatures, or over 1 / the body's size (its box's
//              diagonal) where both are flatter, so that two nearly flat
//              faces are not told apart by noise.
// The worst value of each along the edge is kept, with where it is. The
// class an edge reaches is the highest whose tolerance all three values up
// to it keep: G2 needs G1 and G0, G1 needs G0. A seam (an edge a closed face
// meets itself at) is measured between its two sides; a free edge (one
// face) has no class. The body is not modified.

#include <string>
#include <vector>

#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The tolerances of the classes: the gap (mm), the angle (radians) and the
// curvature difference as a share (0.02: 2 %).
struct ContinuityTolerances {
  double gap = 0.01;
  double angle = 0.1 * 3.14159265358979323846 / 180.0;
  double curvature = 0.02;
};

// The class an edge reaches within the tolerances.
enum class ContinuityClass {
  None, // the faces do not touch within the gap tolerance
  G0,   // they touch
  G1,   // and are tangent
  G2,   // and curvature continuous
  // Nothing to compare: a free edge (one face), or an edge none of whose
  // samples could be measured (a normal undefined on a face all along it).
  Unmeasured,
};

// What is between the faces at a body's edge.
enum class EdgeSides {
  Shared, // two faces
  Seam,   // one closed face, both its sides
  Open,   // one face
};

struct EdgeContinuity {
  int edge = -1;    // the edge's index in the body
  std::string name; // its name; empty when a face at it has none
  EdgeSides sides = EdgeSides::Shared;
  ContinuityClass reached = ContinuityClass::Unmeasured;
  // Samples measured: with a normal on both faces (none at a pole).
  int samples = 0;
  // The worst values along the edge and where they are.
  double gap = 0.0;
  double angle = 0.0;
  double curvature = 0.0;       // 1/mm
  double curvature_share = 0.0; // of the larger curvature, see above
  gp_Pnt gap_at;
  gp_Pnt angle_at;
  gp_Pnt curvature_at;
  // The edge's points at the samples, in order along it, for drawing it.
  std::vector<gp_Pnt> points;
};

// The edges of the faces (indices in the body), each once in index order,
// degenerate edges (a pole's) left out.
std::vector<int> edges_of_faces(const Shape& body, const std::vector<int>& faces);

// The continuity across each edge (indices in the body; every edge of the
// body when empty, degenerate ones left out), measured at `samples` + 1
// points along it, its ends included (`samples` from 1 to 1000), and the
// class it reaches within `tolerances`.
std::vector<EdgeContinuity> edge_continuity(const Shape& body, const std::vector<int>& edges, int samples,
                                            const ContinuityTolerances& tolerances = ContinuityTolerances());

// The class of measured values within the tolerances (Unmeasured for a
// free edge's, or without samples).
ContinuityClass continuity_class(const EdgeContinuity& measured, const ContinuityTolerances& tolerances);

} // namespace mitcad::geometry
