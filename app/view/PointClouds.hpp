// SPDX-License-Identifier: MIT
#pragma once

// Point clouds as the view draws them (mitcad#2240, app/COMMANDS.md "Point
// Clouds"): the model's `point_cloud_points` answer read into the points
// from the cloud's origin (single precision) and the transform that places
// them in the design (double precision), so a scan far from the design's
// origin keeps its detail on the screen. Plain Qt: the window
// (MainWindowPointClouds.cpp), the view (PointCloudView.cpp) and the unit
// tests share it.

#include <array>
#include <cstddef>
#include <cstdint>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad {

struct PointCloudDrawn {
  QString name;
  // x, y, z per point from the origin (mm).
  std::vector<float> points;
  // r, g, b per point; empty: the cloud has no colours (drawn in one).
  std::vector<std::uint8_t> colors;
  // Row by row; the translation includes the origin.
  std::array<std::array<double, 3>, 3> linear{{{1, 0, 0}, {0, 1, 0}, {0, 0, 1}}};
  std::array<double, 3> translation{};
  // The points a shown Point Cloud Deviation did not measure (mitcad#2265):
  // drawn dimmed (ColorRole::DeviationUnmeasured), without colours.
  bool unmeasured = false;
  std::size_t count() const { return points.size() / 3; }
};

// What a shown Point Cloud Deviation measured of its cloud (mitcad#2265),
// from its overlay (pointCloudDeviationOverlayJson): every `step`-th point,
// `measured` per sampled one (false where its distance failed). Empty
// (no cloud) when the overlay is no deviation's.
struct DeviationSample {
  QString cloud;
  std::size_t step = 1;
  std::vector<bool> measured;
  bool operator==(const DeviationSample& other) const {
    return cloud == other.cloud && step == other.step && measured == other.measured;
  }
  bool operator!=(const DeviationSample& other) const { return !(*this == other); }
};
DeviationSample deviationSampleOf(const QJsonObject& overlay);
// The cloud's points the deviation did not measure, as their own cloud to
// draw dimmed (`unmeasured`, no colours); no points when it measured all.
PointCloudDrawn unmeasuredPoints(const PointCloudDrawn& cloud, const DeviationSample& sample);

// The answer of `point_cloud_points` read; a cloud without points when the
// answer is damaged.
PointCloudDrawn pointCloudDrawnOf(const QJsonObject& answer);
// The names of the shown clouds of the `point_clouds` query, in order.
QStringList shownPointClouds(const QJsonArray& entries);
// For UI tests, when it changes: "Point clouds drawn: PointCloud1 120
// points, Scan 4 points coloured", or "Point clouds drawn: none"; a
// deviation's points it did not measure "PointCloud1 21 points not
// measured" (mitcad#2265).
QString pointCloudsLog(const std::vector<PointCloudDrawn>& drawn);

} // namespace mitcad
