// SPDX-License-Identifier: MIT
#pragma once

// Curvature continuous (G2) corners of fillets (mitcad#2332): where
// curvature continuous roundings meet other roundings at a vertex, every
// edge there rounded. The roundings stop at planes across their edges, a
// setback from the vertex: the corner is first cut back to those planes (a
// notch taken away at a convex corner, filled at a concave one), so that
// each rounding ends on a planar face as a chain of its own does; then the
// notch's faces are replaced by a patch of one polynomial (B-spline) face
// per face at the vertex, curvature continuous to the faces and the
// roundings around it and to each other. Internal to the geometry library.

#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// A rounded edge at the corner's vertex: how far from the edge its rounding
// touches the faces, and whether it is curvature continuous.
struct CornerLeg {
  int edge = -1; // of the body
  double contact = 0.0;
  bool curvature = true;
};

// A corner to build: its vertex, whether its edges are convex (else all
// concave), and how far along each edge from the vertex the roundings stop.
struct G2Corner {
  std::string vertex; // the vertex's name in the body
  gp_Pnt apex;
  bool convex = true;
  double distance = 0.0;
  // The least setback the corner can be built with (a little more than the
  // farthest its roundings' contact lines meet on a face), and whether the
  // setback was asked for (a setback corner) rather than worked out.
  double least = 0.0;
  bool asked = false;
  struct Leg {
    std::string edge; // the edge's name in the body
    gp_Dir toward;    // along the edge towards the vertex
    double length = 0.0;
    bool curvature = true;
  };
  std::vector<Leg> legs;
  // The faces at the vertex, their normals out of the material.
  std::vector<gp_Dir> normals;

  // The plane across leg i where its rounding ends, its normal towards the
  // vertex.
  gp_Pln plane(std::size_t i) const;
};

// The corner at a vertex of the body where the legs (every edge there) are
// rounded, its roundings set back `setback` from the vertex, or for 0 by
// 1.5 times the farthest the roundings' contact lines meet on a face, at
// most 0.9 of its shortest edge. Throws std::invalid_argument in words
// where it cannot be built: fewer than three edges, a face that is not
// planar, an edge that is not straight, convex and concave edges together,
// a face whose corner is a half turn or wider, a setback the roundings
// reach past or an edge shorter than it.
G2Corner g2_corner(const Shape& body, int vertex, const std::vector<CornerLeg>& legs, double setback);

// The setbacks of corners at both ends of an edge brought within 0.9 of
// it, the worked out ones shortened (not below their least); throws
// std::invalid_argument in words where they cannot be.
void share_edges(std::vector<G2Corner>& corners);

// The body with each corner's notch taken away (convex) or filled (concave):
// the region between the vertex and the planes across its edges, its new
// faces named <feature>:corner(<vertex>), one in each plane. Throws
// std::invalid_argument where other faces of the body lie within a notch.
ShapePtr notch_corners(const std::string& feature, const Shape& body, const std::vector<G2Corner>& corners);

// The faces <feature>:corner(<vertex>) of `rounded` (the notched body with
// its edges rounded, each rounding ending in its plane) replaced by the
// corner patch: one face per face at the vertex, all named
// <feature>:corner(<vertex>), between seams from the middles of the
// roundings' end sections (cut there) to a middle point. Each is a B-spline
// surface, the Boolean sum of quintic Hermite interpolations of its four
// sides' position, first and second derivative across, that takes on the
// position, the tangent plane and the curvature across its boundary of the
// faces and roundings outside (the side's curve swept along its edge, as
// they are there) and of the patch's other faces along the seams; G2 all
// round, but only tangent continuous across the lines from where a
// tangent continuous rounding's arc meets its faces. Where a tangent
// continuous rounding meets the corner, the result says so in an
// information.
ShapePtr g2_corner_patches(const std::string& feature, const Shape& rounded, const std::vector<G2Corner>& corners);

} // namespace mitcad::geometry::detail
