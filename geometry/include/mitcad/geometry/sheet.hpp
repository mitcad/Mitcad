// SPDX-License-Identifier: MIT
#pragma once

// Sheet material construction (mitcad#206). Curves and domain identities are
// authoritative; the kernel does not infer a sheet from an arbitrary B-rep.
// This first constructor accepts coplanar, consistently oriented panels.
// Cylindrical patches and paired tangent ports are added by the bend family.

#include <cstddef>
#include <cstdint>
#include <string>
#include <variant>
#include <vector>

#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/sheet_domain.hpp"

namespace mitcad::geometry {

enum class SheetReferenceSide { Front, Back };

struct SheetPlanarPatch {
  std::string feature;
  std::uint32_t slot = 0;
  Frame frame;
  // Segment names encode stable source-curve lineage as a face role key.
  Region region;
};

struct SheetShapeSpec {
  double thickness = 0.0;
  SheetReferenceSide reference_side = SheetReferenceSide::Front;
  std::vector<SheetPlanarPatch> planar_patches;
  double tolerance = 1.0e-7;
};

struct SheetSolid {
  ShapePtr shape;
  // Indices into the input patches, including every domain of a joined body.
  std::vector<std::size_t> domains;
};

struct SheetGeometry {
  std::vector<SheetSolid> solids;
  std::vector<std::string> diagnostics;
};

// Front-to-back follows the frame normal. Front reference: offsets [0,t];
// back reference: [-t,0]. Closed curves retain their exact support geometry.
// Invalid/open domains, non-coplanar patches and inconsistent frames fail.
// Islands remain separate solids; touching panels retain all domain aliases.
SheetGeometry sheet_shape(const SheetShapeSpec& spec);

enum class MaterialParity { Positive, Negative };

struct PlaneEmbedding {
  Frame front_frame;
  MaterialParity front_to_back_sign = MaterialParity::Positive;
};

struct CylinderEmbedding {
  gp_Pnt centre;
  gp_Dir axis{0, 0, 1};
  gp_Dir radial_start{1, 0, 0};
  gp_Dir angular_direction{0, 1, 0};
  double inside_radius = 0;
  double neutral_radius = 0;
  double angle_extent = 0;
  bool inner_is_front = true;
};

// A conical panel (mitcad#2435): its chart is the development of the
// neutral cone about its apex. Chart point (x, y) lies hypot(x, y) from the
// apex along the generator, at the developed angle atan2(y, x) (taken in
// [-w, 2 pi - w), w = pi - angle_extent / 2), which is the angle
// atan2(y, x) / sin(half_angle) about the axis from radial_start towards
// angular_direction. The material reaches neutral_depth inside the neutral
// cone and thickness - neutral_depth outside it. Not joined by ports.
struct ConeEmbedding {
  gp_Pnt apex;
  gp_Dir axis{0, 0, 1};
  gp_Dir radial_start{1, 0, 0};
  gp_Dir angular_direction{0, 1, 0};
  double half_angle = 0;
  double neutral_depth = 0;
  double angle_extent = 0;
  bool inner_is_front = true;
};

// An oblique (eccentric) conical panel (mitcad#2491): the cone of the lines
// from `apex` through the circle about `centre` of `radius` in the plane of
// radial_start and angular_direction (at right angles), the angle round
// the circle from radial_start towards angular_direction. `apex` is the
// front surface's; the material lies along the cone's normals from it,
// thickness deep, away from the cone's inside when inner_is_front and
// towards it otherwise. The chart is the development of the neutral
// surface (neutral_depth from the inner one) by its generators
// (sheet_oblique_cone.hpp): chart(theta, rho) = base(theta) + rho
// (cos phi, sin phi), rho along the generator from the apex. The material
// lies within the angles [-w, angle_extent + w], w = pi - angle_extent / 2
// (angle_extent less than a turn). Not joined by ports.
struct ObliqueConeEmbedding {
  gp_Pnt apex;
  gp_Pnt centre;
  double radius = 0;
  gp_Dir radial_start{1, 0, 0};
  gp_Dir angular_direction{0, 1, 0};
  double neutral_depth = 0;
  double angle_extent = 0;
  bool inner_is_front = true;
};

struct SheetPatch {
  SheetDomainRef domain;
  std::vector<MaterialRegion> regions;
  std::variant<PlaneEmbedding, CylinderEmbedding, ConeEmbedding, ObliqueConeEmbedding> embedding;
};

struct ShapePortSide {
  BoundaryPieceRef boundary;
  ParamSpan span;
  AxialMap axial;
};

struct ShapePort {
  std::uint32_t token = 0;
  ParamSpan axial_interval;
  ShapePortSide first;
  ShapePortSide second;
};

struct SheetChartShapeSpec {
  double thickness = 0;
  std::vector<SheetPatch> patches;
  std::vector<ShapePort> ports;
  double tolerance = 1.0e-7;
};

struct RegionMembership {
  SheetDomainRef domain;
  std::size_t region = 0;
};

struct SheetChartSolid {
  ShapePtr shape;
  std::vector<RegionMembership> membership;
};

struct SheetChartGeometry {
  std::vector<SheetChartSolid> solids;
  std::vector<std::string> diagnostics;
};

// Build the present configuration from immutable named material charts.
// Plane origins already identify the front surface: no reference offset is
// applied here. Cylinders keep exact supporting surfaces and paired walls.
SheetChartGeometry sheet_shape(const SheetChartShapeSpec& spec);

} // namespace mitcad::geometry
