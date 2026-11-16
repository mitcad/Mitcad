// SPDX-License-Identifier: MIT
// A feature preview's bodies (mitcad#266): which bodies the preview of
// CommandSession::previewFromReport draws and where, from the model's
// preview report and the bodies the view shows, for a synthetic design with
// components placed off the origin, hidden bodies and occurrences and a
// linked component. The reports are those the model gives for such a design
// (core/model-tests/api/src/preview_placement_tests.rs checks the model's side).
#include "UnitTests.hpp"
#include "framework/PreviewBodies.hpp"

#include <cmath>
#include <cstdio>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "preview_bodies.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

// A translation as the model writes placements: 4 rows of 4.
QJsonArray shift(double x, double y, double z) {
  return QJsonArray{QJsonArray{1.0, 0.0, 0.0, x}, QJsonArray{0.0, 1.0, 0.0, y}, QJsonArray{0.0, 0.0, 1.0, z},
                    QJsonArray{0.0, 0.0, 0.0, 1.0}};
}

// A placement's translation is (x, y, z); an empty placement is the
// identity.
bool at(const QJsonArray& rows, double x, double y, double z) {
  const double place[3] = {x, y, z};
  for (int row = 0; row < 3; ++row) {
    const double value = rows.isEmpty() ? 0.0 : rows.at(row).toArray().at(3).toDouble();
    if (std::abs(value - place[row]) > 1e-9) {
      return false;
    }
  }
  return true;
}

QJsonObject bodyRow(const char* uid, bool changed) {
  return {{QStringLiteral("uid"), QString::fromLatin1(uid)},
          {QStringLiteral("name"), QString::fromLatin1(uid)},
          {QStringLiteral("changed"), changed}};
}

QJsonObject newPair(const char* body, const char* occurrence, const QJsonArray& transform, bool visible = true) {
  return {{QStringLiteral("body"), QString::fromLatin1(body)},
          {QStringLiteral("occurrence"), QString::fromLatin1(occurrence)},
          {QStringLiteral("transform"), transform},
          {QStringLiteral("visible"), visible}};
}

QJsonObject report(const QJsonArray& bodies, const QJsonArray& instances = {}) {
  QJsonObject result{{QStringLiteral("status"), QStringLiteral("ok")},
                     {QStringLiteral("bodies"), bodies},
                     {QStringLiteral("removed"), QJsonArray()},
                     {QStringLiteral("tool"), true}};
  if (!instances.isEmpty()) {
    result.insert(QStringLiteral("instances"), instances);
  }
  return result;
}

const PreviewBody* entryOf(const std::vector<PreviewBody>& bodies, const char* uid, const char* occurrence) {
  for (const PreviewBody& entry : bodies) {
    if (entry.uid == QLatin1String(uid) && entry.occurrence == QLatin1String(occurrence)) {
      return &entry;
    }
  }
  return nullptr;
}

bool anyOf(const std::vector<PreviewBody>& bodies, const char* uid) {
  for (const PreviewBody& entry : bodies) {
    if (entry.uid == QLatin1String(uid)) {
      return true;
    }
  }
  return false;
}

// No body of a component (an occurrence) is drawn at the design's origin.
bool noComponentBodyAtTheOrigin(const std::vector<PreviewBody>& bodies) {
  for (const PreviewBody& entry : bodies) {
    if (!entry.occurrence.isEmpty() && at(entry.transform, 0, 0, 0)) {
      return false;
    }
  }
  return true;
}

// The design:
//   root:  F2.b0, a block at the origin;
//   Board:1 (O1, Board placed at 60, 0, 0): F4.b0 shown, F5.b0 a hidden
//          body;
//   Board:2 (O5, at 60, 30, 0): the same bodies, F4.b0 shown;
//   Cover:1 (O2, at 0, 0, 40), a hidden occurrence: F7.b0;
//   Part:1 (O3, a linked component at 0, 80, 0): F8.b0 from the linked
//          file's base feature; Part:2 (O4, at 0, -80, 0) hidden.
// The view shows what the instances query lists as visible.
std::vector<ShownBody> shownBodies() {
  return {
      {QStringLiteral("F2.b0"), QString(), QJsonArray()},
      {QStringLiteral("F4.b0"), QStringLiteral("O1"), shift(60, 0, 0)},
      {QStringLiteral("F4.b0"), QStringLiteral("O5"), shift(60, 30, 0)},
      {QStringLiteral("F8.b0"), QStringLiteral("O3"), shift(0, 80, 0)},
  };
}

// Extrude on a root sketch, a new body F9.b0 in the root: the hidden body,
// the hidden occurrence and the hidden linked occurrence stay hidden; what
// is shown keeps its placement; the new body is in the root.
void rootExtrudeTests() {
  const std::vector<ShownBody> shown = shownBodies();
  const auto bodies = previewBodies(
      report({bodyRow("F2.b0", false), bodyRow("F4.b0", false), bodyRow("F5.b0", false), bodyRow("F7.b0", false),
              bodyRow("F8.b0", false), bodyRow("F9.b0", true)},
             {newPair("F9.b0", "", shift(0, 0, 0))}),
      shown);
  CHECK(bodies.size() == 5);
  // Hidden in the normal view: not in the preview.
  CHECK(!anyOf(bodies, "F5.b0")); // a hidden body of Board
  CHECK(!anyOf(bodies, "F7.b0")); // the body of a hidden occurrence
  CHECK(entryOf(bodies, "F8.b0", "O4") == nullptr); // the hidden linked occurrence
  // Unchanged, shown: where the view shows it, with its look.
  const PreviewBody* board = entryOf(bodies, "F4.b0", "O1");
  CHECK(board && at(board->transform, 60, 0, 0) && !board->placed && !board->made && !board->previewShape);
  CHECK(board && board->shown == 1);
  const PreviewBody* copy = entryOf(bodies, "F4.b0", "O5");
  CHECK(copy && at(copy->transform, 60, 30, 0) && copy->shown == 2);
  const PreviewBody* linked = entryOf(bodies, "F8.b0", "O3");
  CHECK(linked && at(linked->transform, 0, 80, 0) && !linked->made && linked->shown == 3);
  const PreviewBody* root = entryOf(bodies, "F2.b0", "");
  CHECK(root && at(root->transform, 0, 0, 0) && !root->made);
  // The new body in the root, at the root's placement.
  const PreviewBody* made = entryOf(bodies, "F9.b0", "");
  CHECK(made && made->made && made->placed && made->previewShape && at(made->transform, 0, 0, 0));
  CHECK(made && made->shown == -1);
  CHECK(noComponentBodyAtTheOrigin(bodies));
}

// A cut that also changes the hidden body and the hidden occurrence's body
// (the cut's targets are every intersecting body): those stay hidden all
// the same; the shown one takes the preview's shape where it is shown.
void hiddenChangedTests() {
  const std::vector<ShownBody> shown = shownBodies();
  const auto bodies = previewBodies(report({bodyRow("F2.b0", true), bodyRow("F4.b0", true), bodyRow("F5.b0", true),
                                            bodyRow("F7.b0", true), bodyRow("F8.b0", false)}),
                                    shown);
  CHECK(bodies.size() == 4);
  CHECK(!anyOf(bodies, "F5.b0"));
  CHECK(!anyOf(bodies, "F7.b0"));
  const PreviewBody* board = entryOf(bodies, "F4.b0", "O1");
  CHECK(board && board->previewShape && !board->made && at(board->transform, 60, 0, 0));
  const PreviewBody* copy = entryOf(bodies, "F4.b0", "O5");
  CHECK(copy && copy->previewShape && at(copy->transform, 60, 30, 0));
  CHECK(noComponentBodyAtTheOrigin(bodies));
}

// A new body made inside Board (Board active): drawn new in each of Board's
// visible occurrences, at their placements; not in the hidden ones, and
// never at the origin. A new body without a placement is not drawn.
void componentNewBodyTests() {
  const std::vector<ShownBody> shown = shownBodies();
  const auto bodies = previewBodies(
      report({bodyRow("F2.b0", false), bodyRow("F4.b0", false), bodyRow("F5.b0", false), bodyRow("F7.b0", false),
              bodyRow("F8.b0", false), bodyRow("F10.b0", true), bodyRow("F11.b0", true), bodyRow("F12.b0", true)},
             {newPair("F10.b0", "O1", shift(60, 0, 0)), newPair("F10.b0", "O5", shift(60, 30, 0)),
              // A new body of the hidden Cover: hidden like Cover's others.
              newPair("F11.b0", "O2", shift(0, 0, 40), false)}),
      shown);
  const PreviewBody* first = entryOf(bodies, "F10.b0", "O1");
  CHECK(first && first->made && first->placed && at(first->transform, 60, 0, 0));
  const PreviewBody* second = entryOf(bodies, "F10.b0", "O5");
  CHECK(second && second->made && at(second->transform, 60, 30, 0));
  CHECK(!anyOf(bodies, "F11.b0"));
  // F12.b0: changed, but no occurrence places it at the marker.
  CHECK(!anyOf(bodies, "F12.b0"));
  CHECK(!anyOf(bodies, "F5.b0"));
  CHECK(bodies.size() == 6);
  CHECK(noComponentBodyAtTheOrigin(bodies));
}

// New occurrences of a component (a component pattern) take the look of
// the shown body; moved occurrences move their bodies; removed pairs and
// removed bodies go; a failed preview draws nothing.
void occurrenceTests() {
  const std::vector<ShownBody> shown = shownBodies();
  QJsonObject moved = report({bodyRow("F2.b0", false), bodyRow("F4.b0", false), bodyRow("F8.b0", false)},
                             {newPair("F4.b0", "O9", shift(60, 60, 0))});
  moved.insert(QStringLiteral("placements"),
               QJsonArray{QJsonObject{{QStringLiteral("path"), QStringLiteral("O3")},
                                      {QStringLiteral("transform"), shift(0, 90, 0)}}});
  moved.insert(QStringLiteral("removed_instances"),
               QJsonArray{QJsonObject{{QStringLiteral("body"), QStringLiteral("F4.b0")},
                                      {QStringLiteral("occurrence"), QStringLiteral("O5")}}});
  const auto bodies = previewBodies(moved, shown);
  const PreviewBody* added = entryOf(bodies, "F4.b0", "O9");
  CHECK(added && added->made && added->shown == 1 && at(added->transform, 60, 60, 0));
  const PreviewBody* linked = entryOf(bodies, "F8.b0", "O3");
  CHECK(linked && linked->placed && !linked->made && at(linked->transform, 0, 90, 0));
  CHECK(entryOf(bodies, "F4.b0", "O5") == nullptr);
  CHECK(entryOf(bodies, "F4.b0", "O1") != nullptr);
  CHECK(bodies.size() == 4);

  // F4.b0 removed by the command: neither of its shown occurrences.
  const auto removed = previewBodies(report({bodyRow("F2.b0", false), bodyRow("F8.b0", false)}), shown);
  CHECK(!anyOf(removed, "F4.b0"));
  CHECK(removed.size() == 2);

  QJsonObject failed = report({bodyRow("F2.b0", true)});
  failed.insert(QStringLiteral("status"), QStringLiteral("failed"));
  CHECK(previewBodies(failed, shown).empty());
}

// A consumed tool (mitcad#561): Board's F4.b0, a tool where Board:1 shows
// it, leaves Board, so the preview has it in neither occurrence; both
// shown pairs are what the preview removes, for the command to draw the
// one it is not picked in as a tool. A body the preview keeps, a hidden
// body and a failed preview give none.
void removedShownTests() {
  const std::vector<ShownBody> shown = shownBodies();
  const auto removed = removedShownBodies(report({bodyRow("F2.b0", true), bodyRow("F5.b0", false),
                                                  bodyRow("F7.b0", false), bodyRow("F8.b0", false)}),
                                          shown);
  CHECK(removed.size() == 2);
  CHECK(removed.size() == 2 && removed[0] == 1 && removed[1] == 2);
  CHECK(removedShownBodies(report({bodyRow("F2.b0", true), bodyRow("F4.b0", false), bodyRow("F8.b0", false)}),
                           shown)
            .empty());
  QJsonObject failed = report({bodyRow("F2.b0", true)});
  failed.insert(QStringLiteral("status"), QStringLiteral("failed"));
  CHECK(removedShownBodies(failed, shown).empty());
}

} // namespace

int previewBodiesTests() {
  failures = 0;
  rootExtrudeTests();
  hiddenChangedTests();
  componentNewBodyTests();
  occurrenceTests();
  removedShownTests();
  return failures;
}
MITCAD_UNIT_TEST(previewBodiesTests);
