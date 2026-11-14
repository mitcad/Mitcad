// SPDX-License-Identifier: MIT
// Toolpaths in the 3D view (mitcad#1013, cam/CamToolpaths.hpp): the work
// frame placing work coordinates in the design, the moves sorted into
// rapids, cuts and entries (a plunge, a ramp, a helix), arcs drawn in
// steps with their sweep and direction, the tool's places for Simulate and
// the log lines the UI tests read.
#include "cam/CamToolpaths.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "cam_toolpaths.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::cam::Point3;

bool same(double a, double b) { return std::abs(a - b) < 1e-6; }

bool same(const Point3& a, const Point3& b) { return same(a[0], b[0]) && same(a[1], b[1]) && same(a[2], b[2]); }

QJsonArray xyz(double x, double y, double z) { return QJsonArray{x, y, z}; }

QJsonObject rapid(double x, double y, double z) {
  return {{QStringLiteral("kind"), QStringLiteral("rapid")}, {QStringLiteral("to"), xyz(x, y, z)}};
}

QJsonObject line(double x, double y, double z) {
  return {{QStringLiteral("kind"), QStringLiteral("line")}, {QStringLiteral("to"), xyz(x, y, z)},
          {QStringLiteral("feed"), 1200}};
}

QJsonObject arc(double x, double y, double z, double cx, double cy, bool ccw) {
  return {{QStringLiteral("kind"), QStringLiteral("arc")}, {QStringLiteral("to"), xyz(x, y, z)},
          {QStringLiteral("center"), QJsonArray{cx, cy}}, {QStringLiteral("ccw"), ccw},
          {QStringLiteral("feed"), 1200}};
}

// The frame of a setup whose work zero is at (10, 20, 30) with its X along
// the design's Y and its Y along the design's -X (turned a quarter about Z).
mitcad::cam::WorkFrame turned() {
  return mitcad::cam::WorkFrame::of({{QStringLiteral("origin"), xyz(10, 20, 30)},
                                     {QStringLiteral("x"), xyz(0, 1, 0)},
                                     {QStringLiteral("y"), xyz(-1, 0, 0)},
                                     {QStringLiteral("z"), xyz(0, 0, 1)}});
}

void frameTests() {
  const mitcad::cam::WorkFrame frame = turned();
  CHECK(frame.valid);
  CHECK(same(frame.toModel({0, 0, 0}), {10, 20, 30}));
  CHECK(same(frame.toModel({5, 0, 0}), {10, 25, 30}));
  CHECK(same(frame.toModel({0, 5, -2}), {5, 20, 28}));
  // A frame the model could not find draws nothing in its place.
  CHECK(!mitcad::cam::WorkFrame::of({{QStringLiteral("error"), QStringLiteral("no stock")}}).valid);
  CHECK(!mitcad::cam::WorkFrame::of({}).valid);
}

void styleTests() {
  using mitcad::cam::MoveStyle;
  using mitcad::cam::styleOf;
  CHECK(styleOf(QStringLiteral("rapid"), {0, 0, 5}, {0, 0, -5}) == MoveStyle::Rapid);
  CHECK(styleOf(QStringLiteral("line"), {0, 0, 5}, {0, 0, -1}) == MoveStyle::Entry);  // a plunge
  CHECK(styleOf(QStringLiteral("line"), {0, 0, 0}, {10, 0, -1}) == MoveStyle::Entry); // a ramp
  CHECK(styleOf(QStringLiteral("arc"), {5, 0, 0}, {5, 0, -1}) == MoveStyle::Entry);   // a helix
  CHECK(styleOf(QStringLiteral("line"), {0, 0, -1}, {10, 0, -1}) == MoveStyle::Feed);
  CHECK(styleOf(QStringLiteral("line"), {0, 0, -1}, {0, 0, 2}) == MoveStyle::Feed); // up at the feed
}

void arcTests() {
  // A quarter counter-clockwise from (5, 0) to (0, 5) about the origin:
  // 18 steps of 5 degrees, every point on the circle, ending at the end.
  const std::vector<Point3> quarter = mitcad::cam::arcPoints({5, 0, 0}, {0, 5, 0}, {0, 0}, true);
  CHECK(quarter.size() == 18);
  CHECK(same(quarter.back(), {0, 5, 0}));
  for (const Point3& p : quarter) {
    CHECK(same(std::hypot(p[0], p[1]), 5.0));
    CHECK(p[0] >= -1e-9 && p[1] >= -1e-9);
  }
  // Clockwise the other way round: three quarters, through negative Y.
  const std::vector<Point3> threeQuarters = mitcad::cam::arcPoints({5, 0, 0}, {0, 5, 0}, {0, 0}, false);
  CHECK(threeQuarters.size() == 54);
  CHECK(threeQuarters.at(17)[1] < 0.0);
  // A full turn down a helix: 72 steps, Z going down evenly.
  const std::vector<Point3> helix = mitcad::cam::arcPoints({5, 0, 0}, {5, 0, -2}, {0, 0}, true);
  CHECK(helix.size() == 72);
  CHECK(same(helix.at(35)[2], -1.0));
  CHECK(same(helix.back(), {5, 0, -2}));
}

void linesTests() {
  // From above the start: a rapid down, a plunge, a cut along X, a
  // quarter arc, a rapid up.
  const QJsonArray moves{rapid(0, 0, 5), rapid(0, 0, 1), line(0, 0, -2), line(10, 0, -2),
                         arc(15, 5, -2, 10, 5, true), rapid(15, 5, 5)};
  std::optional<Point3> at;
  const mitcad::cam::PathLines lines = mitcad::cam::pathLines(moves, turned(), at);
  CHECK(lines.moves == 6);
  CHECK(lines.rapidMoves == 3); // the first one only places the start
  CHECK(lines.entryMoves == 1);
  CHECK(lines.feedMoves == 2);
  CHECK(lines.rapids.size() == 12);
  CHECK(lines.entries.size() == 6);
  CHECK(lines.feeds.size() == 6 + 18 * 6);
  CHECK(at && same(*at, {15, 5, 5}));
  // The plunge in the design: from (0, 0, 1) to (0, 0, -2) of the work.
  CHECK(same(lines.entries.at(2), 31.0) && same(lines.entries.at(5), 28.0));
  // The cut along the work's X runs along the design's Y.
  CHECK(same(lines.feeds.at(3), 10.0) && same(lines.feeds.at(4), 30.0));
  CHECK(mitcad::cam::pathLinesLog(QStringLiteral("Contour 1"), lines) ==
        QStringLiteral("Contour 1: 6 moves (rapid 3, feed 2, entry 1), 22 segments"));

  // The next operation goes on from where the last one left the tool.
  std::optional<Point3> next = at;
  const mitcad::cam::PathLines more = mitcad::cam::pathLines(QJsonArray{rapid(0, 0, 5)}, turned(), next);
  CHECK(more.rapidMoves == 1 && more.rapids.size() == 6);
}

void playbackTests() {
  const QJsonArray moves{rapid(0, 0, 5), line(0, 0, -1), arc(10, 0, -1, 5, 0, true)};
  std::optional<Point3> at;
  std::vector<mitcad::cam::PlaybackPoint> points;
  mitcad::cam::appendPlayback(moves, turned(), 2, at, points);
  // The start, the plunge's end, the half turn's 36 steps.
  CHECK(points.size() == 1 + 1 + 36);
  CHECK(points.front().operation == 2 && points.front().move == 0);
  CHECK(points.back().move == 2);
  CHECK(same(points.back().at, turned().toModel({10, 0, -1})));
}

void sceneTests() {
  mitcad::cam::ToolpathScene scene;
  CHECK(scene.empty());
  CHECK(scene.log() == QStringLiteral("CAM toolpath overlay: none"));
  scene.groups.push_back({mitcad::cam::ToolpathScene::Look::Rapid, std::vector<float>(12, 0.0f)});
  scene.groups.push_back({mitcad::cam::ToolpathScene::Look::Feed, std::vector<float>(18, 0.0f)});
  scene.groups.push_back({mitcad::cam::ToolpathScene::Look::Entry, std::vector<float>(6, 0.0f)});
  CHECK(!scene.empty());
  CHECK(scene.log() ==
        QStringLiteral("CAM toolpath overlay: 3 groups, 6 segments (rapid 2, feed 3, entry 1, stale 0)"));
  scene.tool = mitcad::cam::ToolpathScene::Tool{{10, 20.5, -1}, {0, 0, 1}, 6.0, 18.0};
  CHECK(scene.log().endsWith(QStringLiteral("; tool at 10, 20.5, -1")));
  CHECK(mitcad::cam::shortNumber(-0.0001) == QStringLiteral("0"));
  CHECK(mitcad::cam::shortNumber(12.5) == QStringLiteral("12.5"));
}

} // namespace

int camToolpathsTests() {
  failures = 0;
  frameTests();
  styleTests();
  arcTests();
  linesTests();
  playbackTests();
  sceneTests();
  return failures;
}
