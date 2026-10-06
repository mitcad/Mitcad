// SPDX-License-Identifier: MIT
#pragma once

// Physical properties: volume, area, mass, centre of mass and inertia.

#include <array>
#include <vector>

#include "mitcad/analysis/common.hpp"

namespace mitcad::analysis {

// Physical properties of one or several bodies.
//
// Densities are g/cm^3 (water 1.0, aluminium 2.70, steel 7.85), masses kg
// and moments of inertia kg mm^2, as the properties dialog shows them.
struct PhysicalProperties {
  double volume = 0.0; // mm^3, closed solids only
  double area = 0.0;   // mm^2, all faces
  double mass = 0.0;   // kg
  // Of the volume; of the surface for bodies without volume. mm.
  Vec3 center_of_mass;
  // Inertia tensor about the centre of mass, in world-parallel axes:
  // diagonal Ixx = integral of (y^2 + z^2) dm, ...; off-diagonal
  // Ixy = -integral of x y dm, ... (the tensor, not the products).
  std::array<std::array<double, 3>, 3> inertia{};
  // Principal moments, ascending, and their axes (unit vectors; any
  // perpendicular axes when moments are equal).
  std::array<double, 3> principal_moments{};
  std::array<Vec3, 3> principal_axes{};
  Bounds bounds;
};

// Properties of a body (any shape; its solids give volume and mass).
// Mesh bodies are measured from their triangulation; a closed mesh (every
// triangle edge shared by two triangles) counts as a solid.
PhysicalProperties physical_properties(const TopoDS_Shape& shape, double density = 1.0);

// Combined properties of several bodies with their own densities
// (`densities` empty: all 1.0; otherwise one per body).
PhysicalProperties physical_properties(const std::vector<TopoDS_Shape>& bodies,
                                       const std::vector<double>& densities = {});

} // namespace mitcad::analysis
