// SPDX-License-Identifier: MIT
#pragma once

#include <array>
#include <cstddef>
#include <optional>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

namespace mitcad {

using DecalPoint = std::array<double, 3>;

// A decal as the view and the renderer draw it (mitcad#2244): an entry of
// the model's `decals` query whose face is there at the marker, with its
// grid on the face (core/model-api/src/commands.md, "Decals"): rows + 1
// rows of columns + 1 points, from the image's top row to its bottom row,
// each from its left column to its right column, with the face's outward
// normal there and the image's coordinates (u from 0 at its left edge to 1
// at its right edge, v from 0 at its top edge to 1 at its bottom edge).
// After the grid's points come the points where the face's edges cut it
// (mitcad#2266); the triangles cover the part of the grid on the face, the
// only part drawn.
struct DecalShown {
  QString name;
  QString sha256;
  QString surface; // plane, cylinder or cone
  QString body;    // the face's body and name, as the model keeps them
  QString face;
  QString occurrence;
  int columns = 1;
  int rows = 1;
  std::vector<DecalPoint> points;
  std::vector<DecalPoint> normals;
  std::vector<std::array<double, 2>> uvs;
  // Indices of points, counter-clockwise seen from outside the material.
  std::vector<std::array<std::size_t, 3>> triangles;
  bool cut = false; // part of it lies off its face
  double width = 0.0; // mm, along the face
  double height = 0.0;
  double angle = 0.0; // degrees
  bool flip = false;
  double opacity = 1.0;
  bool visible = true;
  int pixelsWide = 0;
  int pixelsHigh = 0;
  // The Decal panel's live result, not a decal of the design yet: the log
  // says so, so that the design's decal drawn the same after OK is a line
  // of its own.
  bool panel = false;

  // The index of the grid's point in row `row` and column `column`.
  std::size_t at(int row, int column) const {
    return static_cast<std::size_t>(row) * static_cast<std::size_t>(columns + 1) + static_cast<std::size_t>(column);
  }
};

// One entry of the query; none when its face is gone (`error`) or its grid
// or triangles are not whole. An entry without triangles is drawn whole.
std::optional<DecalShown> decalOf(const QJsonObject& entry);
// The entries the view draws, in order: the shown ones with their faces.
std::vector<DecalShown> shownDecals(const QJsonArray& entries);
// The decal whose triangles hold `point` (within `tolerance` mm of it), the
// last drawn first; none when no decal is drawn there.
std::optional<QString> decalAt(const std::vector<DecalShown>& decals, const DecalPoint& point, double tolerance);
// For UI tests, when it changes: "Decals drawn: Decal1 10.0 x 5.0 mm on a
// plane 100%, Decal2 20.0 x 10.0 mm on a cylinder turned 90 deg flipped
// 50% (8 x 1)" (" cut" after the opacity when its face's edges cut it, the
// grid's pieces for a curved face; the panel's live result ends with " in
// the panel"), or "Decals drawn: none".
QString decalsLog(const std::vector<DecalShown>& shown);

} // namespace mitcad
