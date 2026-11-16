// SPDX-License-Identifier: MIT
// The mount frames of library parts as the app reads them (mitcad#525):
// a manifest's `mounts` block (each name once, the kind and joint
// defaults), their labels and the cells of a grid placement's answer
// (core/model-tests/joints/src/mount_tests.rs checks the model's side).
#include "UnitTests.hpp"
#include "library/MountFrames.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "mount_frames.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QJsonArray parse(const char* text) { return QJsonDocument::fromJson(QByteArray(text)).array(); }

} // namespace

int mountFramesTests() {
  const QVector<MountFrameInfo> frames = mountFramesOf(parse(
      R"([{"name": "foot", "kind": "foot", "joint": "rigid", "footprint": [2, 1], "pitch": [42, 42]},
          {"name": "base"},
          {"name": "foot", "kind": "face"},
          {"kind": "face"},
          {"name": "head", "kind": "shaft", "joint": "cylindrical"}])"));
  CHECK(frames.size() == 3);
  CHECK(frames.value(0).name == QStringLiteral("foot"));
  CHECK(frames.value(0).kind == QStringLiteral("foot"));
  CHECK(frames.value(0).raw.value(QStringLiteral("footprint")).toArray().size() == 2);
  // Defaults: a face frame, fixed.
  CHECK(frames.value(1).kind == QStringLiteral("face"));
  CHECK(frames.value(1).joint == QStringLiteral("rigid"));
  CHECK(mountFrameLabel(frames.value(1)) == QStringLiteral("base (on a face, fixed)"));
  CHECK(mountFrameLabel(frames.value(2)) == QStringLiteral("head (into a hole, turns and slides)"));
  CHECK(mountTakesAxes(QStringLiteral("shaft")));
  CHECK(mountTakesAxes(QStringLiteral("hole")));
  CHECK(!mountTakesAxes(QStringLiteral("foot")));
  CHECK(mountJointText(QStringLiteral("planar")) == QStringLiteral("free on the face"));
  CHECK(mountKindText(QStringLiteral("grid")) == QStringLiteral("a grid"));
  // Nothing listed, or no block at all.
  CHECK(mountFramesOf(QJsonValue()).isEmpty());
  CHECK(mountFramesOf(parse("[]")).isEmpty());
  // The cells of a grid placement's answer.
  CHECK(mountCellsText(parse("[[1, 0]]")) == QStringLiteral("(1, 0)"));
  CHECK(mountCellsText(parse("[[0, 0], [0, 1]]")) == QStringLiteral("(0, 0) and (0, 1)"));
  CHECK(mountCellsText(parse("[[0, 0], [1, 0], [2, 0]]")) == QStringLiteral("(0, 0), (1, 0) and (2, 0)"));
  CHECK(mountCellsText(QJsonArray()).isEmpty());
  return failures;
}
MITCAD_UNIT_TEST(mountFramesTests);
