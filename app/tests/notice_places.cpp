// SPDX-License-Identifier: MIT
// The places of the model's notices (mitcad#1327): read from a result's
// notices each once, the messages of a level in plain words, the places
// in the log's words; and a feature of the timeline keeps its notices'
// places and a failure's plain error. A hover's log words, also while a
// job computes (mitcad#1600), and the feature a message under the pointer
// gives when another takes its place (mitcad#1609).
#include "browser/DocumentSnapshot.hpp"
#include "framework/NoticePlaces.hpp"

#include <cstdio>
#include <memory>
#include <utility>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>

#include "framework/Command.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "notice_places.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QJsonObject parsed(const char* json) { return QJsonDocument::fromJson(QByteArray(json)).object(); }

void readTests() {
  const QJsonObject preview = parsed(R"json({"status": "ok", "warnings": ["the fillet of edge E{a|b} hangs"],
    "notices": [
      {"level": "warning", "message": "The fillet hangs 0.06 mm past the edge of the face beside it.",
       "places": [{"kind": "face", "body": "F1.b0", "name": "F3:fillet(E{a|b})"},
                  {"kind": "point", "at": [1, 2.5, -3]}]},
      {"level": "warning", "message": "A face beside it is turned over.",
       "places": [{"kind": "face", "body": "F1.b0", "name": "F3:fillet(E{a|b})"},
                  {"kind": "edge", "name": "E{c|d}"}]},
      {"level": "info", "message": "built 0.1 % smaller"}]})json");
  const QJsonArray places = noticePlaces(preview.value(QStringLiteral("notices")).toArray());
  // Each once, in order.
  CHECK(places.size() == 3);
  CHECK(placesText(places) ==
        QStringLiteral("face F3:fillet(E{a|b}) of F1.b0, point 1.000,2.500,-3.000, edge E{c|d}"));
  CHECK(placesText(QJsonArray()) == QStringLiteral("none"));
  // A region of a sketch (mitcad#1334): its sketch where it is known.
  const QJsonObject regions = parsed(R"json({"notices": [
      {"level": "warning", "message": "a region found again after a change in Sketch1",
       "places": [{"kind": "region", "sketch": "F1", "name": "r{c1,c2}"}, {"kind": "region", "name": "r{c3}"}]}]})json");
  CHECK(placesText(noticePlaces(regions.value(QStringLiteral("notices")).toArray())) ==
        QStringLiteral("region r{c1,c2} of F1, region r{c3}"));
  // The panel's message: the warnings' plain words, not the model's text.
  const QStringList warnings = noticeMessages(preview, QStringLiteral("warning"));
  CHECK(warnings.size() == 2);
  CHECK(warnings.first().startsWith(QStringLiteral("The fillet hangs")));
  CHECK(noticeMessages(preview, QStringLiteral("error")).isEmpty());
  CHECK(noticeMessages(QJsonObject(), QStringLiteral("warning")).isEmpty());
}

// A model that answers the timeline query only.
class TimelineModel : public CommandContext {
public:
  explicit TimelineModel(QJsonObject timeline) : m_timeline(std::move(timeline)) {}

  QJsonValue query(const QJsonObject& query) const override {
    if (query.value(QStringLiteral("query")).toString() == QStringLiteral("timeline")) {
      return m_timeline;
    }
    return QJsonValue();
  }
  std::shared_ptr<geometry::Shape> bodyShape(const QString&) const override { return nullptr; }
  TopoDS_Shape itemShape(const SelectionItem&) const override { return TopoDS_Shape(); }
  TopoDS_Shape analysisShape(const QJsonObject&) const override { return TopoDS_Shape(); }

private:
  QJsonObject m_timeline;
};

void snapshotTests() {
  const TimelineModel model(parsed(R"json({"marker": 2, "features": [
    {"uid": "F2", "name": "Fillet1", "type": "fillet", "status": "warning", "level": "warning",
     "notices": [{"level": "warning", "message": "The corner of the fillet folds back on itself.",
                  "target": "F2", "places": [{"kind": "face", "body": "F1.b0", "name": "F2:corner(V{a|b|c})"}]}]},
    {"uid": "F3", "name": "Fillet2", "type": "fillet", "status": "error", "level": "error",
     "error": "fillet failed at V{a|b|c}",
     "notices": [{"level": "error", "message": "fillet failed at a vertex", "target": "F3",
                  "places": [{"kind": "vertex", "name": "V{a|b|c}"}]}]}]})json"));
  const DocumentSnapshot snapshot = DocumentSnapshot::read(model);
  const DocumentSnapshot::Feature* warned = snapshot.feature(QStringLiteral("F2"));
  CHECK(warned != nullptr);
  if (warned != nullptr) {
    CHECK(warned->places.size() == 1);
    CHECK(warned->error == QStringLiteral("The corner of the fillet folds back on itself."));
  }
  const DocumentSnapshot::Feature* failed = snapshot.feature(QStringLiteral("F3"));
  CHECK(failed != nullptr);
  if (failed != nullptr) {
    // The error in plain words; its place kept.
    CHECK(failed->error == QStringLiteral("fillet failed at a vertex"));
    CHECK(placesText(failed->places) == QStringLiteral("vertex V{a|b|c}"));
  }
}

// The log's words for a hover (mitcad#1600: while a job computes, the
// places kept and not counted).
void hoverTextTests() {
  const QJsonObject result = parsed(R"json({"notices": [{"level": "warning", "message": "m",
    "places": [{"kind": "face", "body": "F1.b0", "name": "F4:fillet(E{a|b})"}, {"kind": "point", "at": [1, 2, 3]}]}]})json");
  const QJsonArray places = noticePlaces(result.value(QStringLiteral("notices")).toArray());
  CHECK(noticeHoverText(QStringLiteral("timeline"), places, 2) ==
        QStringLiteral("timeline: face F4:fillet(E{a|b}) of F1.b0, point 1.000,2.000,3.000 (2 drawn)"));
  CHECK(noticeHoverText(QStringLiteral("timeline"), places, -1) ==
        QStringLiteral("timeline: face F4:fillet(E{a|b}) of F1.b0, point 1.000,2.000,3.000 (drawn when the job ends)"));
  CHECK(noticeHoverText(QStringLiteral("panel"), places, 0) ==
        QStringLiteral("panel: face F4:fillet(E{a|b}) of F1.b0, point 1.000,2.000,3.000 (0 drawn)"));
  CHECK(noticeHoverText(QStringLiteral("panel"), QJsonArray(), 3) == QStringLiteral("none"));
  CHECK(noticeHoverText(QStringLiteral("panel"), QJsonArray(), -1) == QStringLiteral("none"));
}

// The feature a message under the pointer gives (mitcad#1609): given
// again when another message takes the hovered one's place.
void hoverLatchTests() {
  NoticeHover hover;
  // Not over it: nothing given, whatever it shows.
  CHECK(!hover.update(false, QStringLiteral("F4")));
  CHECK(hover.feature().isEmpty());
  // Over a message about no feature: nothing.
  CHECK(!hover.update(true, QString()));
  // The pointer over Rim's warning: F4, once, however often it moves.
  CHECK(hover.update(true, QStringLiteral("F4")));
  CHECK(hover.feature() == QStringLiteral("F4"));
  CHECK(!hover.update(true, QStringLiteral("F4")));
  // Another feature's message in its place, the pointer where it was.
  CHECK(hover.update(true, QStringLiteral("F7")));
  CHECK(hover.feature() == QStringLiteral("F7"));
  // A message about no feature in its place: none.
  CHECK(hover.update(true, QString()));
  CHECK(hover.feature().isEmpty());
  CHECK(!hover.update(true, QString()));
  // A feature's message again, then the pointer leaves: none, once.
  CHECK(hover.update(true, QStringLiteral("F4")));
  CHECK(hover.update(false, QStringLiteral("F4")));
  CHECK(hover.feature().isEmpty());
  CHECK(!hover.update(false, QStringLiteral("F4")));
}

} // namespace

int noticePlacesTests() {
  failures = 0;
  readTests();
  snapshotTests();
  hoverTextTests();
  hoverLatchTests();
  return failures;
}
