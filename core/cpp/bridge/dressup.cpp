// SPDX-License-Identifier: MIT
#include "bridge/dressup.hpp"

#include <stdexcept>
#include <string>
#include <vector>

#include "mitcad/geometry/dressup.hpp"
#include "mitcad_bridge/kernel/dressup.h"

namespace mitcad::bridge {
namespace {

std::vector<std::string> names(const rust::Vec<rust::String>& list) {
  std::vector<std::string> result;
  for (const rust::String& name : list) {
    result.emplace_back(name);
  }
  return result;
}

} // namespace

std::shared_ptr<geometry::Shape> fillet(rust::Str feature, const geometry::Shape& body,
                                        rust::Slice<const FilletSet> sets,
                                        bool rolling_ball_corners) {
  std::vector<geometry::FilletSet> converted;
  for (const FilletSet& set : sets) {
    geometry::FilletSet out;
    out.edges = names(set.edges);
    out.faces = names(set.faces);
    out.tangent_chain = set.tangent_chain;
    out.curvature = set.curvature;
    out.weight = set.weight;
    switch (set.kind) {
    case FilletKind::Constant:
      out.size = geometry::FilletSize::Constant;
      out.radius = set.value;
      break;
    case FilletKind::ChordLength:
      out.size = geometry::FilletSize::ChordLength;
      out.chord = set.value;
      break;
    case FilletKind::Variable:
      out.size = geometry::FilletSize::Variable;
      out.radius = set.value;
      out.radius2 = set.value2;
      out.start_vertex = std::string(set.start_vertex);
      for (std::size_t i = 0; i + 1 < set.mid.size(); i += 2) {
        out.mid.emplace_back(set.mid[i], set.mid[i + 1]);
      }
      break;
    case FilletKind::Asymmetric:
      out.size = geometry::FilletSize::Asymmetric;
      out.radius = set.value;
      out.radius2 = set.value2;
      out.reference_face = std::string(set.reference_face);
      out.flip = set.flip;
      break;
    default:
      throw std::invalid_argument("unknown fillet size");
    }
    converted.push_back(std::move(out));
  }
  return geometry::fillet(std::string(feature), body, converted, rolling_ball_corners);
}

std::shared_ptr<geometry::Shape> chamfer(rust::Str feature, const geometry::Shape& body,
                                         rust::Slice<const ChamferSet> sets, ChamferCornerKind corner) {
  std::vector<geometry::ChamferSet> converted;
  for (const ChamferSet& set : sets) {
    geometry::ChamferSet out;
    out.edges = names(set.edges);
    out.faces = names(set.faces);
    out.tangent_chain = set.tangent_chain;
    out.spec.distance = set.distance;
    out.spec.flip = set.flip;
    out.spec.reference_face = std::string(set.reference_face);
    switch (set.kind) {
    case ChamferKind::EqualDistance:
      out.spec.type = geometry::ChamferType::EqualDistance;
      break;
    case ChamferKind::TwoDistances:
      out.spec.type = geometry::ChamferType::TwoDistances;
      out.spec.distance2 = set.value2;
      break;
    case ChamferKind::DistanceAngle:
      out.spec.type = geometry::ChamferType::DistanceAngle;
      out.spec.angle = set.value2;
      break;
    default:
      throw std::invalid_argument("unknown chamfer type");
    }
    converted.push_back(std::move(out));
  }
  geometry::ChamferCorner shape = geometry::ChamferCorner::Chamfer;
  switch (corner) {
  case ChamferCornerKind::Chamfer:
    break;
  case ChamferCornerKind::Miter:
    shape = geometry::ChamferCorner::Miter;
    break;
  case ChamferCornerKind::Blend:
    shape = geometry::ChamferCorner::Blend;
    break;
  default:
    throw std::invalid_argument("unknown chamfer corner type");
  }
  return geometry::chamfer(std::string(feature), body, converted, shape);
}

rust::Vec<rust::String> shape_notes(const geometry::Shape& shape) {
  rust::Vec<rust::String> notes;
  for (const std::string& note : shape.notes()) {
    notes.push_back(rust::String(note));
  }
  return notes;
}

} // namespace mitcad::bridge
