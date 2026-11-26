// SPDX-License-Identifier: MIT
// A lathe's zones and its turret's body as Simulate draws them
// (mitcad#2412, cam/CamToolpaths.hpp `operationEntry`, `zoneLines`,
// `turretLines`): an operation's entry found by its id; a zone as a box of
// twelve edges, one without depth in Y as a rectangle of four; the turret's
// body as a rectangle round the tool's point, along the given X axis or
// away from the spindle axis through the tool; in the design's coordinates
// by the setup's work frame; the tool's work point kept with its places.
#include "UnitTests.hpp"
#include "cam/CamToolpaths.hpp"

#include <cmath>
#include <cstddef>
#include <cstdio>
#include <optional>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "cam_machine_drawn.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool closeTo(double a, double b) { return std::abs(a - b) < 1e-4; }

// A segment's ends (`s`: its index) are `a` and `b`, either way round.
bool segmentIs(const std::vector<float>& lines, std::size_t s, const mitcad::cam::Point3& a,
               const mitcad::cam::Point3& b) {
  const float* v = lines.data() + s * 6;
  const auto at = [v](int k, const mitcad::cam::Point3& p) {
    return closeTo(v[k], p[0]) && closeTo(v[k + 1], p[1]) && closeTo(v[k + 2], p[2]);
  };
  return (at(0, a) && at(3, b)) || (at(0, b) && at(3, a));
}

QJsonObject zone(const QString& name, const QJsonArray& min, const QJsonArray& max) {
  return {{QStringLiteral("name"), name},
          {QStringLiteral("moving"), false},
          {QStringLiteral("min"), min},
          {QStringLiteral("max"), max}};
}

} // namespace

int camMachineDrawnTests() {
  failures = 0;
  using mitcad::cam::operationEntry;
  using mitcad::cam::turretLines;
  using mitcad::cam::WorkFrame;
  using mitcad::cam::zoneLines;
  // An operation's entry by its id; none when not listed.
  const QJsonArray list{QJsonObject{{QStringLiteral("id"), 2}, {QStringLiteral("x"), 1}},
                        QJsonObject{{QStringLiteral("id"), 7}, {QStringLiteral("x"), 2}}};
  CHECK(operationEntry(list, 7).value(QStringLiteral("x")).toInt() == 2);
  CHECK(operationEntry(list, 3).isEmpty());
  CHECK(operationEntry(QJsonArray(), 2).isEmpty());
  // The work frame 100 mm up the design's Z, the spindle axis along it.
  WorkFrame frame;
  frame.origin = {0.0, 0.0, 100.0};
  frame.valid = true;
  // A box: twelve edges; a zone without depth in Y: four.
  const QJsonArray zones{
      zone(QStringLiteral("Tailstock"), QJsonArray{100.0, -10.0, -40.0}, QJsonArray{120.0, 10.0, -30.0}),
      zone(QStringLiteral("Door"), QJsonArray{150.0, 0.0, -100.0}, QJsonArray{160.0, 0.0, 300.0})};
  const std::vector<float> lines = zoneLines(zones, frame);
  CHECK(lines.size() == static_cast<std::size_t>((12 + 4) * 6));
  // The box's first edge along X at its lowest Y and Z, in the design
  // 100 mm up; the door's first edge at Y 0.
  CHECK(segmentIs(lines, 0, {100.0, -10.0, 60.0}, {120.0, -10.0, 60.0}));
  CHECK(segmentIs(lines, 12, {150.0, 0.0, 0.0}, {160.0, 0.0, 0.0}));
  // A zone without its bounds is left out; none draws nothing.
  CHECK(zoneLines(QJsonArray{QJsonObject{{QStringLiteral("name"), QStringLiteral("Bad")}}}, frame).empty());
  CHECK(zoneLines(QJsonArray(), frame).empty());
  // The turret's body along X for turning: 0 to 80 mm out from the tool at
  // (10, 0, -5), 10 mm towards the main spindle and 20 away from it.
  const QJsonObject turning{{QStringLiteral("x"), QJsonArray{0.0, 80.0}},
                            {QStringLiteral("z"), QJsonArray{-10.0, 20.0}},
                            {QStringLiteral("x_axis"), QJsonArray{1.0, 0.0}}};
  const std::vector<float> box = turretLines(turning, {10.0, 0.0, -5.0}, frame);
  CHECK(box.size() == static_cast<std::size_t>(4 * 6));
  CHECK(segmentIs(box, 0, {10.0, 0.0, 85.0}, {90.0, 0.0, 85.0}));
  CHECK(segmentIs(box, 1, {90.0, 0.0, 85.0}, {90.0, 0.0, 115.0}));
  // Without an X axis: away from the spindle axis through the tool, here
  // along Y.
  QJsonObject face = turning;
  face.insert(QStringLiteral("x_axis"), QJsonValue(QJsonValue::Null));
  const std::vector<float> radial = turretLines(face, {0.0, 6.0, 0.0}, frame);
  CHECK(segmentIs(radial, 0, {0.0, 6.0, 90.0}, {0.0, 86.0, 90.0}));
  // A plane's X axis turned round in Y on the sub-spindle.
  QJsonObject plane = turning;
  plane.insert(QStringLiteral("x_axis"), QJsonArray{0.0, -1.0});
  const std::vector<float> turned = turretLines(plane, {3.0, -4.0, 0.0}, frame);
  CHECK(segmentIs(turned, 0, {3.0, -4.0, 90.0}, {3.0, -84.0, 90.0}));
  // No body: nothing.
  CHECK(turretLines(QJsonObject(), {0.0, 0.0, 0.0}, frame).empty());
  // The tool's places keep their work point.
  std::vector<mitcad::cam::PlaybackPoint> points;
  std::optional<mitcad::cam::Point3> at;
  const QJsonArray moves{QJsonObject{{QStringLiteral("kind"), QStringLiteral("rapid")},
                                     {QStringLiteral("to"), QJsonArray{5.0, 0.0, 2.0}}}};
  mitcad::cam::appendPlayback(moves, frame, 0, at, points);
  CHECK(points.size() == 1);
  CHECK(!points.empty() && closeTo(points.front().work[0], 5.0) && closeTo(points.front().work[2], 2.0) &&
        closeTo(points.front().at[2], 102.0));
  return failures;
}
MITCAD_UNIT_TEST(camMachineDrawnTests);
