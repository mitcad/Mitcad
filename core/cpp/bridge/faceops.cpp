// SPDX-License-Identifier: MIT
#include "bridge/faceops.hpp"

#include <stdexcept>
#include <string>
#include <vector>

#include "bridge/profile.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/split.hpp"
#include "mitcad_bridge/kernel/faceops.h"

namespace mitcad::bridge {
namespace {

std::vector<std::string> names(rust::Slice<const rust::String> list) {
  std::vector<std::string> result;
  for (const rust::String& name : list) {
    result.emplace_back(name);
  }
  return result;
}

std::shared_ptr<geometry::Shape> first_shape(const ShapeList& shapes) {
  if (shapes.size() == 0 || !shapes.at(0)) {
    throw std::invalid_argument("the tool has no shape");
  }
  return shapes.at(0);
}

// The geometry library's tool; curves need the sketch frame and regions.
geometry::Tool to_tool(const ToolSpec& spec, const ShapeList& shapes, const Frame* frame,
                       rust::Slice<const Region> regions) {
  // Only planes and curves have a direction (a zero vector is no gp_Dir).
  const auto normal = [&spec] { return gp_Dir(spec.normal[0], spec.normal[1], spec.normal[2]); };
  switch (spec.kind) {
  case ToolKind::Plane:
    return geometry::Tool::of_plane(
        gp_Pln(gp_Pnt(spec.origin[0], spec.origin[1], spec.origin[2]), normal()));
  case ToolKind::Face:
    return geometry::Tool::of_face(first_shape(shapes), std::string(spec.face));
  case ToolKind::Body:
    return geometry::Tool::of_body(first_shape(shapes));
  case ToolKind::Curves: {
    if (frame == nullptr) {
      throw std::invalid_argument("sketch curves cannot be used here");
    }
    geometry::Tool tool;
    tool.kind = geometry::Tool::Kind::Curves;
    tool.frame = to_geometry(*frame);
    tool.regions = to_geometry(regions);
    for (const rust::String& curve : spec.curves) {
      tool.curves.emplace_back(curve);
    }
    tool.direction = normal();
    return tool;
  }
  default:
    break;
  }
  throw std::invalid_argument("unknown tool kind");
}

} // namespace

std::shared_ptr<geometry::Shape> shell(rust::Str feature, const geometry::Shape& body,
                                       rust::Slice<const rust::String> faces,
                                       const ShellParams& params) {
  geometry::ShellSpec spec;
  spec.faces = names(faces);
  spec.inside = params.inside;
  spec.outside = params.outside;
  spec.tangent_chain = params.tangent_chain;
  spec.rounded = params.rounded;
  return geometry::shell(std::string(feature), body, spec);
}

std::shared_ptr<geometry::Shape> draft(rust::Str feature, const geometry::Shape& body,
                                       rust::Slice<const rust::String> faces, const ToolSpec& plane,
                                       const ShapeList& plane_shapes, const DraftParams& params) {
  geometry::DraftSpec spec;
  spec.faces = names(faces);
  spec.plane = to_tool(plane, plane_shapes, nullptr, {});
  spec.angle = params.angle;
  if (params.two_sided) {
    spec.angle2 = params.angle2;
  }
  spec.flip = params.flip;
  spec.tangent_chain = params.tangent_chain;
  return geometry::draft(std::string(feature), body, spec);
}

std::shared_ptr<geometry::Shape> offset_faces(rust::Str feature, const geometry::Shape& body,
                                              rust::Slice<const rust::String> faces,
                                              double distance) {
  return geometry::offset_faces(std::string(feature), body, names(faces), distance);
}

std::shared_ptr<geometry::Shape> delete_faces(rust::Str feature, const geometry::Shape& body,
                                              rust::Slice<const rust::String> faces) {
  return geometry::delete_faces(std::string(feature), body, names(faces));
}

std::shared_ptr<geometry::Shape> replace_faces(rust::Str feature, const geometry::Shape& body,
                                               rust::Slice<const rust::String> faces,
                                               const ToolSpec& target,
                                               const ShapeList& target_shapes, bool tangent_chain) {
  return geometry::replace_faces(std::string(feature), body, names(faces),
                                 to_tool(target, target_shapes, nullptr, {}), tangent_chain);
}

std::unique_ptr<ShapeList> split_body(rust::Str feature, const geometry::Shape& body,
                                      const ToolSpec& tool, const ShapeList& tool_shapes,
                                      const Frame& frame, rust::Slice<const Region> regions) {
  auto pieces = std::make_unique<ShapeList>();
  for (const auto& piece : geometry::split_body(std::string(feature), body,
                                                to_tool(tool, tool_shapes, &frame, regions),
                                                tool.extend)) {
    pieces->push(piece);
  }
  return pieces;
}

std::shared_ptr<geometry::Shape> split_faces(rust::Str feature, const geometry::Shape& body,
                                             rust::Slice<const rust::String> faces,
                                             const ToolSpec& tool, const ShapeList& tool_shapes,
                                             const Frame& frame, rust::Slice<const Region> regions) {
  return geometry::split_faces(std::string(feature), body, names(faces),
                               to_tool(tool, tool_shapes, &frame, regions), tool.extend);
}

} // namespace mitcad::bridge
