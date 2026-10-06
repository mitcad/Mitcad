// SPDX-License-Identifier: MIT
#include "mitcad/analysis/properties.hpp"

#include <algorithm>
#include <cmath>
#include <numeric>
#include <string>
#include <vector>

#include <GProp_GProps.hxx>
#include <GProp_PrincipalProps.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Mat.hxx>

#include "convert.hpp"

namespace mitcad::analysis {

namespace {

// kg per mm^3 for a density in g/cm^3.
constexpr double kKgPerMm3 = 1e-6;

struct Accumulated {
  GProp_GProps mass;    // weighted by density, kg and kg mm^2
  bool has_mass = false;
  GProp_GProps volumes; // unweighted, for the centre when nothing has mass
  GProp_GProps surface; // unweighted, for bodies without volume
  double volume = 0.0;
  double area = 0.0;
  Bounds bounds;
};

void add_bounds(Bounds& total, const Bounds& part) {
  if (part.empty) {
    return;
  }
  if (total.empty) {
    total = part;
    return;
  }
  total.min = {std::min(total.min.x, part.min.x), std::min(total.min.y, part.min.y),
               std::min(total.min.z, part.min.z)};
  total.max = {std::max(total.max.x, part.max.x), std::max(total.max.y, part.max.y),
               std::max(total.max.z, part.max.z)};
}

void add_body(Accumulated& total, const TopoDS_Shape& shape, double density) {
  if (shape.IsNull()) {
    throw Error("cannot measure a null shape");
  }
  if (!(density >= 0.0)) {
    throw Error("density must not be negative");
  }
  const bool mesh = detail::has_mesh_faces(shape);
  std::vector<TopoDS_Shape> volumes;
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    volumes.push_back(it.Current());
  }
  // A closed mesh body (STL, OBJ) encloses a volume without being a solid.
  if (volumes.empty() && mesh && detail::closed_mesh(shape)) {
    volumes.push_back(shape);
  }
  for (const TopoDS_Shape& solid : volumes) {
    const bool closed_only = solid.ShapeType() == TopAbs_SOLID;
    GProp_GProps volume = volume_properties(solid, closed_only);
    if (volume.Mass() < 0.0) {
      // An inside-out solid: measure it the right way round.
      volume = volume_properties(solid.Reversed(), closed_only);
    }
    if (volume.Mass() <= 0.0) {
      continue;
    }
    total.volume += volume.Mass();
    total.volumes.Add(volume);
    // OCCT refuses zero weights.
    if (density > 0.0) {
      total.mass.Add(volume, density * kKgPerMm3);
      total.has_mass = true;
    }
  }
  const GProp_GProps surface = surface_properties(shape);
  total.area += surface.Mass();
  total.surface.Add(surface);
  add_bounds(total.bounds, bounds(shape));
}

PhysicalProperties finish(const Accumulated& total) {
  PhysicalProperties result;
  result.volume = total.volume;
  result.area = total.area;
  result.bounds = total.bounds;
  if (total.volume <= 0.0) {
    if (total.area > 0.0) {
      result.center_of_mass = detail::vec(total.surface.CentreOfMass());
    }
    return result;
  }
  // Zero density everywhere: the centre of the volume, no inertia.
  if (!total.has_mass) {
    result.center_of_mass = detail::vec(total.volumes.CentreOfMass());
    return result;
  }
  result.mass = total.mass.Mass();
  result.center_of_mass = detail::vec(total.mass.CentreOfMass());
  const gp_Mat inertia = total.mass.MatrixOfInertia();
  for (int i = 0; i < 3; ++i) {
    for (int j = 0; j < 3; ++j) {
      result.inertia[static_cast<std::size_t>(i)][static_cast<std::size_t>(j)] =
          inertia.Value(i + 1, j + 1);
    }
  }
  const GProp_PrincipalProps principal = total.mass.PrincipalProperties();
  double moments[3] = {0.0, 0.0, 0.0};
  principal.Moments(moments[0], moments[1], moments[2]);
  const gp_Vec axes[3] = {principal.FirstAxisOfInertia(), principal.SecondAxisOfInertia(),
                          principal.ThirdAxisOfInertia()};
  std::array<int, 3> order{0, 1, 2};
  std::sort(order.begin(), order.end(), [&](int a, int b) { return moments[a] < moments[b]; });
  for (std::size_t k = 0; k < 3; ++k) {
    const int i = order[k];
    result.principal_moments[k] = moments[i];
    const double length = axes[i].Magnitude();
    result.principal_axes[k] = length > 0.0 ? detail::vec(axes[i].XYZ() / length) : Vec3{};
  }
  return result;
}

} // namespace

PhysicalProperties physical_properties(const TopoDS_Shape& shape, double density) {
  Accumulated total;
  add_body(total, shape, density);
  return finish(total);
}

PhysicalProperties physical_properties(const std::vector<TopoDS_Shape>& bodies,
                                       const std::vector<double>& densities) {
  if (!densities.empty() && densities.size() != bodies.size()) {
    throw Error("expected one density per body, got " + std::to_string(densities.size()) +
                " for " + std::to_string(bodies.size()) + " bodies");
  }
  Accumulated total;
  for (std::size_t i = 0; i < bodies.size(); ++i) {
    add_body(total, bodies[i], densities.empty() ? 1.0 : densities[i]);
  }
  return finish(total);
}

} // namespace mitcad::analysis
