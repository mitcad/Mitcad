// SPDX-License-Identifier: MIT
// The sub-spindle as Simulate draws it (mitcad#2405,
// cam/CamToolpaths.hpp `subSpindleLines`, `subSpindleFace`): its outline
// revolved about the work Z with its jaws' face where cam_simulate has it
// as each operation cuts, a circle at each point and four lines between a
// part's points, in the design's coordinates by the setup's work frame;
// an operation whose place is not known draws nothing.
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
    std::fprintf(stderr, "cam_sub_drawn.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool closeTo(double a, double b) { return std::abs(a - b) < 1e-4; }

// An outline of one part from `a` to `b` (`[behind, radius]`); appended,
// as a one-element brace list of a QJsonArray would copy it.
QJsonArray outlineOf(const QJsonArray& a, const QJsonArray& b) {
  QJsonArray outline;
  outline.append(QJsonArray{a, b});
  return outline;
}

// The chuck 100 mm across and 10 deep; operations 3 (at the grip, -5) and
// 1 (where it waits is not known).
QJsonObject subSpindle() {
  return {{QStringLiteral("outline"), outlineOf(QJsonArray{0.0, 50.0}, QJsonArray{10.0, 50.0})},
          {QStringLiteral("operations"),
           QJsonArray{QJsonObject{{QStringLiteral("id"), 1}, {QStringLiteral("face"), QJsonValue(QJsonValue::Null)}},
                      QJsonObject{{QStringLiteral("id"), 3}, {QStringLiteral("face"), -5.0}}}}};
}

} // namespace

int camSubDrawnTests() {
  failures = 0;
  using mitcad::cam::subSpindleFace;
  using mitcad::cam::subSpindleLines;
  using mitcad::cam::WorkFrame;
  const QJsonObject sub = subSpindle();
  // Where its jaws' face is: known for operation 3, not for 1, nor for an
  // operation not listed.
  CHECK(subSpindleFace(sub, 3) == std::optional<double>(-5.0));
  CHECK(!subSpindleFace(sub, 1).has_value());
  CHECK(!subSpindleFace(sub, 7).has_value());
  CHECK(!subSpindleFace(QJsonObject(), 3).has_value());
  // The work frame 100 mm up the design's Z, the spindle axis along it.
  WorkFrame frame;
  frame.origin = {0.0, 0.0, 100.0};
  frame.valid = true;
  const std::vector<float> lines = subSpindleLines(sub, -5.0, frame);
  // Two circles of 24 segments and four lines between them: the first
  // circle, the lines, the second circle.
  CHECK(lines.size() == static_cast<std::size_t>((24 * 2 + 4) * 6));
  // The first circle at the jaws' face (design Z 95), 50 mm from the axis;
  // the second 10 mm behind it.
  bool front = true;
  bool back = true;
  for (int s = 0; s < 24; ++s) {
    const float* v = lines.data() + s * 6;
    front = front && closeTo(std::hypot(v[0], v[1]), 50.0) && closeTo(v[2], 95.0) && closeTo(v[5], 95.0);
  }
  for (int s = 28; s < 52; ++s) {
    const float* v = lines.data() + s * 6;
    back = back && closeTo(std::hypot(v[0], v[1]), 50.0) && closeTo(v[2], 105.0);
  }
  CHECK(front);
  CHECK(back);
  // The first line along the chuck: from (50, 0, 95) to (50, 0, 105).
  const float* line = lines.data() + 24 * 6;
  CHECK(closeTo(line[0], 50.0) && closeTo(line[1], 0.0) && closeTo(line[2], 95.0));
  CHECK(closeTo(line[3], 50.0) && closeTo(line[4], 0.0) && closeTo(line[5], 105.0));
  // A point on the axis has no circle; no outline draws nothing.
  const QJsonObject nose{{QStringLiteral("outline"), outlineOf(QJsonArray{-12.0, 0.0}, QJsonArray{0.0, 7.0})}};
  CHECK(subSpindleLines(nose, 0.0, frame).size() == static_cast<std::size_t>((24 + 4) * 6));
  CHECK(subSpindleLines(QJsonObject(), 0.0, frame).empty());
  // A scene with only the sub-spindle drawn is not empty.
  mitcad::cam::ToolpathScene scene;
  CHECK(scene.empty());
  scene.machine = lines;
  CHECK(!scene.empty());
  return failures;
}
MITCAD_UNIT_TEST(camSubDrawnTests);
