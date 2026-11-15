// SPDX-License-Identifier: MIT
#include "bridge/form.hpp"

#include <cstddef>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <gp_Pnt.hxx>

#include "mitcad/geometry/form.hpp"
#include "mitcad_bridge/kernel/form.h"

namespace mitcad::bridge {
namespace {

// The most poles along one side of a patch the bridge passes on.
constexpr std::size_t kMostPoles = 1000;

std::vector<int> counts(const rust::Vec<std::uint32_t>& values) {
  std::vector<int> result;
  result.reserve(values.size());
  for (const std::uint32_t value : values) {
    result.push_back(static_cast<int>(value));
  }
  return result;
}

geometry::SurfacePatch patch_of(const FormPatch& patch) {
  if (patch.nu > kMostPoles || patch.nv > kMostPoles || patch.poles.size() != 3 * patch.nu * patch.nv) {
    throw std::invalid_argument("form: a patch's poles do not match its counts");
  }
  geometry::SurfacePatch result;
  result.name = std::string(patch.face);
  result.u_degree = static_cast<int>(patch.u_degree);
  result.v_degree = static_cast<int>(patch.v_degree);
  result.nu = static_cast<int>(patch.nu);
  result.nv = static_cast<int>(patch.nv);
  result.poles.reserve(patch.nu * patch.nv);
  for (std::size_t k = 0; k + 2 < patch.poles.size(); k += 3) {
    result.poles.emplace_back(patch.poles[k], patch.poles[k + 1], patch.poles[k + 2]);
  }
  result.u_knots.assign(patch.u_knots.begin(), patch.u_knots.end());
  result.v_knots.assign(patch.v_knots.begin(), patch.v_knots.end());
  result.u_mults = counts(patch.u_mults);
  result.v_mults = counts(patch.v_mults);
  return result;
}

} // namespace

std::shared_ptr<geometry::Shape> patch_body(rust::Slice<const FormPatch> patches, bool closed, double tolerance) {
  std::vector<geometry::SurfacePatch> converted;
  converted.reserve(patches.size());
  for (const FormPatch& patch : patches) {
    converted.push_back(patch_of(patch));
  }
  return geometry::patch_body(converted, closed, tolerance);
}

} // namespace mitcad::bridge
