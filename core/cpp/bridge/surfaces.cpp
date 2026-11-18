// SPDX-License-Identifier: MIT
#include "bridge/surfaces.hpp"

#include <cstddef>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include "bridge/profile.hpp"
#include "bridge/sweeps.hpp"
#include "mitcad/geometry/surfaces.hpp"
#include "mitcad_bridge/kernel/surfaces.h"

namespace mitcad::bridge {
namespace {

geometry::Continuity continuity(std::uint8_t g) {
  switch (g) {
  case 0:
    return geometry::Continuity::G0;
  case 1:
    return geometry::Continuity::G1;
  case 2:
    return geometry::Continuity::G2;
  default:
    throw std::invalid_argument("unknown continuity " + std::to_string(g));
  }
}

std::vector<gp_Pnt> points(const rust::Vec<double>& xyz) {
  std::vector<gp_Pnt> result;
  for (std::size_t i = 0; i + 2 < xyz.size(); i += 3) {
    result.emplace_back(xyz[i], xyz[i + 1], xyz[i + 2]);
  }
  return result;
}

} // namespace

std::unique_ptr<ShapeList> patch_regions(rust::Str feature, const Frame& frame,
                                         rust::Slice<const Region> regions) {
  auto patches = std::make_unique<ShapeList>();
  for (const auto& patch :
       geometry::patch_regions(std::string(feature), to_geometry(frame), to_geometry(regions))) {
    patches->push(patch);
  }
  return patches;
}

rust::Vec<OpenBoundaryData> open_boundaries(const geometry::Shape& body) {
  rust::Vec<OpenBoundaryData> result;
  for (const geometry::OpenBoundary& boundary : geometry::open_boundaries(body)) {
    OpenBoundaryData data;
    for (const std::string& face : boundary.faces) {
      data.faces.push_back(rust::String(face));
    }
    for (const gp_Pnt& p : boundary.points) {
      data.points.push_back(p.X());
      data.points.push_back(p.Y());
      data.points.push_back(p.Z());
    }
    data.closed = boundary.closed;
    result.push_back(std::move(data));
  }
  return result;
}

std::shared_ptr<geometry::Shape> patch_loop(rust::Str feature, const geometry::Shape& body,
                                            const PatchLoopData& spec, rust::Slice<const SweepCurve> curves) {
  geometry::PatchLoop loop;
  for (const rust::String& edge : spec.edges) {
    loop.edges.emplace_back(edge);
  }
  // Free edges beside the named ones (mitcad#1955).
  for (const PatchOpening& opening : spec.openings) {
    geometry::OpeningEdges picked;
    picked.index = opening.index;
    for (const rust::String& face : opening.faces) {
      picked.faces.emplace_back(face);
    }
    for (std::size_t i = 0; i + 2 < opening.at.size(); i += 3) {
      picked.at.emplace_back(opening.at[i], opening.at[i + 1], opening.at[i + 2]);
    }
    loop.openings.push_back(std::move(picked));
  }
  loop.opening = spec.opening;
  loop.continuity = continuity(spec.continuity);
  if (spec.edge_continuity_edges.size() != spec.edge_continuity.size()) {
    throw std::invalid_argument("patch: a continuity is needed for each edge that has its own");
  }
  for (std::size_t i = 0; i < spec.edge_continuity.size(); ++i) {
    loop.edge_continuity.emplace_back(std::string(spec.edge_continuity_edges[i]),
                                      continuity(spec.edge_continuity[i]));
  }
  loop.points = points(spec.points);
  // Sketch curves in the loop beside the edges (mitcad#1972).
  for (const SweepCurve& curve : curves) {
    loop.curves.push_back(path_curve_of(curve));
  }
  return geometry::patch_loop(std::string(feature), body, loop);
}

std::shared_ptr<geometry::Shape> stitch(const ShapeList& shapes, double tolerance) {
  std::vector<const geometry::Shape*> inputs;
  for (std::size_t i = 0; i < shapes.size(); ++i) {
    const std::shared_ptr<geometry::Shape> shape = shapes.at(i);
    if (!shape) {
      throw std::invalid_argument("stitch: a body has no shape");
    }
    inputs.push_back(shape.get());
  }
  return geometry::stitch(inputs, tolerance);
}

std::shared_ptr<geometry::Shape> thicken(rust::Str feature, const geometry::Shape& sheet,
                                         double thickness, bool symmetric) {
  return geometry::thicken(std::string(feature), sheet, thickness, symmetric);
}

} // namespace mitcad::bridge
