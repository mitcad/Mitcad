// SPDX-License-Identifier: MIT
#include "bridge/press_pull.hpp"

#include <string>
#include <vector>

#include "mitcad/geometry/press_pull.hpp"
#include "mitcad_bridge/kernel/press_pull.h"

namespace mitcad::bridge {

std::shared_ptr<geometry::Shape> press_pull(rust::Str feature, const geometry::Shape& body,
                                            rust::Slice<const rust::String> faces, const PressPullParams& params) {
  geometry::PressPullSpec spec;
  spec.feature = std::string(feature);
  for (const rust::String& face : faces) {
    spec.faces.emplace_back(face);
  }
  spec.run = params.run == PressPullRun::Along ? geometry::PressPullRun::Along : geometry::PressPullRun::Straight;
  spec.distance = params.distance;
  switch (params.taper) {
  case PressPullTaper::Draft:
    spec.taper = geometry::PressPullTaper::Draft;
    break;
  case PressPullTaper::Scale:
    spec.taper = geometry::PressPullTaper::Scale;
    break;
  default:
    spec.taper = geometry::PressPullTaper::None;
    break;
  }
  spec.taper_angle = params.taper_angle;
  spec.end_scale = params.end_scale;
  return geometry::press_pull(body, spec);
}

FaceRunInfo face_run(const geometry::Shape& body, rust::Str face) {
  const geometry::FaceRun run = geometry::face_run(body, std::string(face));
  FaceRunInfo info{};
  info.centre = {run.centre.X(), run.centre.Y(), run.centre.Z()};
  info.normal = {run.normal.X(), run.normal.Y(), run.normal.Z()};
  switch (run.along) {
  case geometry::FaceRun::Along::Straight:
    info.along = FaceAlong::Straight;
    break;
  case geometry::FaceRun::Along::Turn:
    info.along = FaceAlong::Turn;
    break;
  case geometry::FaceRun::Along::None:
    info.along = FaceAlong::Refused;
    break;
  }
  info.direction = {run.direction.X(), run.direction.Y(), run.direction.Z()};
  const gp_Pnt& origin = run.axis.Location();
  const gp_Dir& axis = run.axis.Direction();
  info.axis_origin = {origin.X(), origin.Y(), origin.Z()};
  info.axis_direction = {axis.X(), axis.Y(), axis.Z()};
  info.radius = run.radius;
  info.reach = run.reach;
  info.refusal = rust::String(run.refusal);
  return info;
}

} // namespace mitcad::bridge
