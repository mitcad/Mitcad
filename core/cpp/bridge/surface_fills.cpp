// SPDX-License-Identifier: MIT
#include "bridge/surface_fills.hpp"

#include <cstddef>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "bridge/sweeps.hpp"
#include "mitcad/geometry/surface_fills.hpp"
#include "mitcad_bridge/kernel/surface_fills.h"

namespace mitcad::bridge {
namespace {

geometry::Continuity continuity_of(std::uint8_t value) {
  switch (value) {
  case 0:
    return geometry::Continuity::G0;
  case 1:
    return geometry::Continuity::G1;
  case 2:
    return geometry::Continuity::G2;
  default:
    throw std::invalid_argument("unknown continuity " + std::to_string(value));
  }
}

// The chain, its body kept alive by `bodies`.
geometry::EdgeChain chain_of(const ChainData& data, const ShapeList& bodies) {
  if (data.body >= bodies.size()) {
    throw std::invalid_argument("a chain's body is not given");
  }
  geometry::EdgeChain chain;
  chain.body = bodies.items()[data.body].get();
  for (const rust::String& edge : data.edges) {
    chain.edges.emplace_back(edge);
  }
  for (const ChainOpening& opening : data.openings) {
    geometry::OpeningEdges edges;
    edges.index = opening.index;
    for (const rust::String& face : opening.faces) {
      edges.faces.emplace_back(face);
    }
    for (std::size_t i = 0; i + 2 < opening.at.size(); i += 3) {
      edges.at.emplace_back(opening.at[i], opening.at[i + 1], opening.at[i + 2]);
    }
    chain.openings.push_back(std::move(edges));
  }
  chain.face = std::string(data.face);
  return chain;
}

geometry::BlendEnd end_of(const BlendEndData& data, const ShapeList& bodies) {
  geometry::BlendEnd end;
  end.chain = chain_of(data.chain, bodies);
  end.continuity = continuity_of(data.continuity);
  end.bulge = data.bulge;
  return end;
}

std::vector<geometry::BoundaryCurve> curves_of(rust::Slice<const BoundaryData> input, const ShapeList& bodies,
                                               rust::Slice<const SweepCurve> curves) {
  std::vector<geometry::BoundaryCurve> result;
  for (const BoundaryData& data : input) {
    geometry::BoundaryCurve curve;
    curve.continuity = continuity_of(data.continuity);
    switch (data.kind) {
    case 0: {
      curve.kind = geometry::BoundaryCurve::Kind::Curves;
      if (data.first > curves.size() || data.count > curves.size() - data.first) {
        throw std::invalid_argument("a boundary's curves run past the curves given");
      }
      for (std::size_t i = data.first; i < data.first + data.count; ++i) {
        curve.curves.push_back(path_curve_of(curves[i]));
      }
      const gp_Pnt origin(data.plane[0], data.plane[1], data.plane[2]);
      curve.plane = gp_Pln(origin, gp_Dir(data.plane[3], data.plane[4], data.plane[5]));
      break;
    }
    case 1:
      curve.kind = geometry::BoundaryCurve::Kind::Edges;
      curve.edges = chain_of(data.chain, bodies);
      break;
    case 2:
      curve.kind = geometry::BoundaryCurve::Kind::Point;
      curve.point = gp_Pnt(data.point[0], data.point[1], data.point[2]);
      break;
    default:
      throw std::invalid_argument("unknown boundary curve kind " + std::to_string(data.kind));
    }
    result.push_back(std::move(curve));
  }
  return result;
}

} // namespace

std::shared_ptr<geometry::Shape> blend_surface(rust::Str feature, const ShapeList& bodies, const BlendEndData& from,
                                               const BlendEndData& to) {
  return geometry::blend_surface(std::string(feature), end_of(from, bodies), end_of(to, bodies));
}

std::shared_ptr<geometry::Shape> boundary_surface(rust::Str feature, const ShapeList& bodies,
                                                  rust::Slice<const SweepCurve> curves, bool fill,
                                                  rust::Slice<const BoundaryData> u,
                                                  rust::Slice<const BoundaryData> v,
                                                  rust::Slice<const BoundaryData> guides) {
  if (fill) {
    return geometry::boundary_fill(std::string(feature), curves_of(u, bodies, curves),
                                   curves_of(guides, bodies, curves));
  }
  return geometry::boundary_network(std::string(feature), curves_of(u, bodies, curves),
                                    curves_of(v, bodies, curves));
}

std::shared_ptr<geometry::Shape> match_surface(const geometry::Shape& body, rust::Str face, const ShapeList& bodies,
                                               const ChainData& target, std::uint8_t continuity) {
  return geometry::match_surface(body, std::string(face), chain_of(target, bodies), continuity_of(continuity));
}

std::shared_ptr<geometry::Shape> fair_surface(const geometry::Shape& body, rust::Slice<const rust::String> faces,
                                              double strength, std::uint32_t iterations, bool keep_edges,
                                              double tolerance) {
  std::vector<std::string> names;
  for (const rust::String& face : faces) {
    names.emplace_back(face);
  }
  geometry::Fairing fairing;
  fairing.strength = strength;
  fairing.iterations = static_cast<int>(iterations);
  fairing.keep_edges = keep_edges;
  fairing.tolerance = tolerance;
  return geometry::fair_surface(body, names, fairing);
}

} // namespace mitcad::bridge
