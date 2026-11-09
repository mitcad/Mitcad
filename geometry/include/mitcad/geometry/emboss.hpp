// SPDX-License-Identifier: MIT
#pragma once

// Emboss: profiles wrapped onto cylinders (mitcad#458). The planar emboss
// is an extrusion (extrude.hpp); on a cylinder the profiles are laid out by
// arc length around it, not projected, and grown radially. On any face they
// can be projected instead (mitcad#1091, projected_emboss).

#include <string>
#include <vector>

#include <gp_Ax1.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The regions of a sketch whose plane is parallel to `axis`, wrapped onto
// the cylinder of `axis` and `radius` and grown radially to `to_radius`
// (larger: out from the axis; smaller: in towards it).
//
// The sketch side's radial direction points from the axis towards the
// sketch plane (the sketch normal for a plane through the axis). A sketch
// point at distance s across the axis (along axis × radial, from the line
// where the axis lies under the sketch) and z along it goes to the angle
// s / radius about the axis from the radial direction, at z: lengths
// around the cylinder of `radius` are the sketch's. The profiles must stay
// within one turn.
//
// Faces are named after `feature`, as an extrusion's:
//   <feature>:side(<segment>)  grown from a profile segment, radially
//   <feature>:start(<region>)  on the cylinder of `radius`
//   <feature>:end(<region>)    on the cylinder of `to_radius`
// Several regions are fused; the result may hold several solids.
//
// On a cone (mitcad#473: `half_angle` not zero), the sketch is the cone's
// development laid against it along the generator under the sketch's
// origin: `axis` runs from the apex where the radius grows, `radius` is the
// cone's radius at the sketch origin's height along the axis (ρ0 = radius
// / sin(half_angle) along the generator from the apex), and `to_radius -
// radius` is how far the profiles grow along the cone's normal (outwards
// when positive). A sketch point at s across the axis and t along it (from
// the origin's height) goes to the point of the cone at the distance
// hypot(s, t + ρ0) from the apex along a generator turned by
// atan2(s, t + ρ0) / sin(half_angle) about the axis: lengths and angles on
// the cone are the sketch's. The walls stand along the cone's normals.
struct EmbossWrapSpec {
  std::string feature;
  Frame frame;
  std::vector<Region> regions;
  gp_Ax1 axis;
  double radius = 0.0;
  double to_radius = 0.0;
  double half_angle = 0.0; // radians: 0 for a cylinder
  // The walls tilted from the normals (mitcad#1547), radians in (-90, 90)
  // degrees: positive widens the profiles away from the face (the top, or
  // a sink's floor, larger than the foot), negative narrows them, as an
  // extrusion's taper. The top region is the foot's moved within the top
  // surface by d tan(taper) (d = |to_radius - radius|) along the boundary's
  // normal in that surface: on the top surface's development (exact on a
  // cylinder and a cone) each edge's curve is offset by OCCT, outwards for
  // a positive taper, the corners where neighbouring offsets meet, and an
  // edge whose corners pass each other drops out (its neighbours meet).
  // Each wall is ruled between its edge on the foot and its moved edge (or
  // that corner) on the top, and the faces are sewn into the solid. A
  // taper that collapses a region (a loop keeps fewer than two edges,
  // neighbours do not meet, the moved edges cross) throws
  // std::invalid_argument naming the region.
  double taper = 0.0;
};

// Throws std::invalid_argument for a sketch plane across the axis, sizes
// that are not positive and profiles that reach around the cylinder, and
// std::runtime_error when the solid cannot be built.
ShapePtr wrapped_emboss(const EmbossWrapSpec& spec);

// Emboss by projection (mitcad#1091): the regions of a sketch projected
// along `direction` (not in the sketch plane; its length does not matter)
// onto the face `face` of the body (every piece the name resolves to), and
// grown from it by `depth` along the face's normals: out of the material
// (the side the face's normal points to) when positive, into it when
// negative. The start is the projected region on the face, the end that
// region offset by `depth`, and the walls stand along the face's normals
// on the region's boundary.
//
// The line through a sketch point along the direction may meet the face
// more than once (a sphere twice): each region takes the piece of the face
// it meets nearest ahead of the sketch plane along the direction, else the
// nearest behind it (the distance measured along the direction's lines,
// whatever their slant to the plane). That piece must cover the region's projection once
// (as the field of the direction through it shows: its flux is the
// region's area across the direction), or the region falls partly off the
// face (or past where it turns away from the direction).
//
// Faces are named after `feature`, as an extrusion's:
//   <feature>:side(<segment>)  a wall, along the face's normals
//   <feature>:start(<region>)  on the face
//   <feature>:end(<region>)    the offset of the face
// Several regions are fused; the result may hold several solids.
struct EmbossProjectSpec {
  std::string feature;
  Frame frame;
  std::vector<Region> regions;
  std::string face;
  gp_Vec direction;
  double depth = 0.0;
  // The walls tilted from the normals (mitcad#1547), radians in (-90, 90)
  // degrees, positive widening as on a wrap (EmbossWrapSpec::taper). The
  // move is measured as the projection sees the profiles: the region is
  // moved in the sketch plane by |depth| tan(taper) (as on a wrap: each
  // edge offset, the corners where the offsets meet), projected onto the
  // face and grown along its normals, and that solid's far face is the
  // top. The foot is the region's own projection, each wall is ruled
  // between a segment's edges on the foot and on the top (or the top's
  // corner where its edge dropped out), and the faces are sewn. A taper
  // that collapses a region throws std::invalid_argument naming it.
  double taper = 0.0;
};

// Throws std::invalid_argument for a face the body lacks, a direction in
// the sketch plane, no depth, and a region that misses the face or falls
// partly off it (the message names the region), and std::runtime_error
// when the solid cannot be built. The body is left as it is.
ShapePtr projected_emboss(const Shape& body, const EmbossProjectSpec& spec);

} // namespace mitcad::geometry
