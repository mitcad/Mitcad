// SPDX-License-Identifier: MIT
#pragma once

// The stock in Simulate (mitcad#1013, phase 3; app/COMMANDS.md "The CNC
// group"): the height grid of the `cam_simulate` query (core/model-api
// commands.md, "CAM: stock simulation") read from its runs, turned into a
// surface in the design by the setup's work frame, shaded by the material
// it leaves over the part, and the snapshots' places along Simulate's
// playback. Plain Qt Core and arithmetic: no view, no model.

#include <cstddef>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

#include "CamToolpaths.hpp"

namespace mitcad::cam {

// The query's `grid`: nx by ny nodes from the origin (work X and Y), a step
// apart, the stock standing on `bottom`, its top before the cut `top`.
struct StockGrid {
  double originX = 0.0;
  double originY = 0.0;
  double stepX = 1.0;
  double stepY = 1.0;
  int nx = 0;
  int ny = 0;
  double bottom = 0.0;
  double top = 0.0;

  static StockGrid of(const QJsonObject& grid);
  bool valid() const { return nx > 0 && ny > 0; }
  std::size_t nodes() const { return static_cast<std::size_t>(nx) * static_cast<std::size_t>(ny); }
};

// Applies runs to the heights (one per node, mm): pairs of a height (µm)
// and a count of nodes, a negative count leaving that many as they are.
// False, the heights unchanged in part, when the runs do not cover the
// nodes exactly.
bool applyRuns(const QJsonArray& runs, std::vector<float>& heights);

// The stock's surface for the 3D view: its top over each cell of the grid
// that holds material (two triangles), and its sides down to its bottom
// along the grid's edges; design coordinates, a normal and a shade per
// vertex.
struct StockMesh {
  std::vector<float> points;  // x, y, z per vertex
  std::vector<float> normals; // x, y, z per vertex
  // Per vertex: 0 the plain stock or nothing left over the part, from 0 to
  // 1 the material left from just over the tolerance to the thickest, -1
  // cut into the part.
  std::vector<float> shades;
  std::vector<int> triangles; // three vertices each, from 0
  bool remaining = false;     // shaded by the material left

  int vertices() const { return static_cast<int>(points.size() / 3); }
  int triangleCount() const { return static_cast<int>(triangles.size() / 3); }
  bool empty() const { return triangles.empty(); }
  // "CAM stock overlay: 2255 vertices, 4320 triangles, plain" (or "by the
  // material left"), "CAM stock overlay: none": the view's log line.
  QString log() const;
};

// The surface of `heights` (one per node). With `part` (the part's heights,
// one per node) the shades say the material left over it: a node more
// than `tolerance` above the part is shaded by its thickness up to
// `thickest`, one more than `tolerance` below the lowest part round it is
// cut into the part.
StockMesh stockMesh(const StockGrid& grid, const std::vector<float>& heights, const WorkFrame& frame,
                    const std::vector<float>* part = nullptr, double tolerance = 0.01, double thickest = 0.0);

// The snapshots' places along a playback of `points` places: at most
// `count`, evenly apart, the last place always one; ascending, each once.
std::vector<int> framePlaces(int points, int count);

// The snapshot to show at a playback place: the last whose place is not
// after it; -1 (the stock before the cut) when there is none.
int frameAt(const std::vector<int>& places, int index);

// "CAM stock: Setup 1, 55 x 41 nodes, 60 snapshots": the log line when
// Simulate reads the stock.
QString stockLog(const QString& setup, const StockGrid& grid, int snapshots);

// "CAM remaining material: Setup 1: max 0.5, mean 0.12, gouge 0, beside 0,
// 3 place(s)": the material left (`remaining` of the query) for the log.
QString remainingLog(const QString& setup, const QJsonObject& remaining);

} // namespace mitcad::cam
