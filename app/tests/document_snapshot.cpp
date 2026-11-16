// SPDX-License-Identifier: MIT
// The document snapshot's feature lookup (mitcad#833): a feature found by
// its uid through the index read() makes, as the browser and the view look
// one up per row, body and profile of a design with thousands of features;
// a missing uid; and a snapshot whose features were added otherwise. A
// feature's remarks (mitcad#1914) mark nothing in the timeline.
#include "UnitTests.hpp"
#include "browser/DocumentSnapshot.hpp"

#include <cstdio>
#include <memory>
#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

#include "framework/Command.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "document_snapshot.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

// A model whose timeline has `count` features F1..F<count>, the even ones
// sketches; every other query has an empty answer.
class TimelineModel : public CommandContext {
public:
  explicit TimelineModel(QJsonObject timeline) : m_timeline(std::move(timeline)) {}
  explicit TimelineModel(int count) {
    QJsonArray features;
    for (int i = 1; i <= count; ++i) {
      features.append(QJsonObject{{QStringLiteral("uid"), QStringLiteral("F%1").arg(i)},
                                  {QStringLiteral("name"), QStringLiteral("Feature%1").arg(i)},
                                  {QStringLiteral("type"), i % 2 == 0 ? QStringLiteral("sketch")
                                                                      : QStringLiteral("extrude")},
                                  {QStringLiteral("status"), QStringLiteral("ok")}});
    }
    m_timeline = QJsonObject{{QStringLiteral("features"), features}, {QStringLiteral("marker"), count}};
  }

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

void readTests() {
  const TimelineModel model(5000);
  const DocumentSnapshot snapshot = DocumentSnapshot::read(model);
  CHECK(snapshot.features.size() == 5000);
  CHECK(snapshot.featureIndex.size() == 5000);
  for (int i = 1; i <= 5000; ++i) {
    const DocumentSnapshot::Feature* found = snapshot.feature(QStringLiteral("F%1").arg(i));
    CHECK(found != nullptr && found->name == QStringLiteral("Feature%1").arg(i) && found->index == i - 1);
  }
  const DocumentSnapshot::Feature* sketch = snapshot.feature(QStringLiteral("F42"));
  CHECK(sketch != nullptr && sketch->isSketch());
  CHECK(snapshot.feature(QStringLiteral("F5001")) == nullptr);
  CHECK(snapshot.feature(QString()) == nullptr);
  // A copy keeps its index with its features.
  const DocumentSnapshot copy = snapshot;
  const DocumentSnapshot::Feature* copied = copy.feature(QStringLiteral("F7"));
  CHECK(copied != nullptr && copied == &copy.features.at(6));
}

void handMadeTests() {
  // Features appended without the index (a test's snapshot) are still found.
  DocumentSnapshot snapshot;
  CHECK(snapshot.feature(QStringLiteral("F1")) == nullptr);
  DocumentSnapshot::Feature feature;
  feature.uid = QStringLiteral("F1");
  feature.name = QStringLiteral("Sketch1");
  snapshot.features.append(feature);
  feature.uid = QStringLiteral("F2");
  feature.name = QStringLiteral("Extrude1");
  snapshot.features.append(feature);
  const DocumentSnapshot::Feature* found = snapshot.feature(QStringLiteral("F2"));
  CHECK(found != nullptr && found->name == QStringLiteral("Extrude1"));
  CHECK(snapshot.feature(QStringLiteral("F3")) == nullptr);
}

// A 2D sketch turned into a 3D sketch in place (mitcad#1544) keeps its uid
// and name; its type, and with it its kind and icon, changes.
void convertedSketchTests() {
  DocumentSnapshot::Feature feature;
  feature.uid = QStringLiteral("F1");
  feature.name = QStringLiteral("Sketch1");
  feature.type = QStringLiteral("sketch");
  CHECK(feature.isSketch() && !feature.isSpatialSketch() && feature.isAnySketch());
  CHECK(featureIcon(feature.type) == QStringLiteral("sketch"));
  feature.type = QStringLiteral("spatial_sketch");
  CHECK(!feature.isSketch() && feature.isSpatialSketch() && feature.isAnySketch());
  CHECK(featureIcon(feature.type) == QStringLiteral("sketch-3d"));
}

// A fillet whose tangent chain stops short (mitcad#1914): the timeline
// gives the remark as an information notice with its places and in its
// `remarks`, its level none. The snapshot's feature is not noted (no
// badge, no tooltip line) and takes neither the remark's message nor its
// places; a warning beside it is kept with its places.
void remarkTests() {
  const QString remark =
      QStringLiteral("The fillet's tangent chain stops where other sharp edges meet: 2 edges that continue "
                     "it are left sharp.");
  QJsonArray edges;
  edges.append(QJsonObject{{QStringLiteral("kind"), QStringLiteral("edge")},
                           {QStringLiteral("body"), QStringLiteral("F2.b0")},
                           {QStringLiteral("name"), QStringLiteral("E{F2:a|F2:b}")}});
  QJsonArray remarks;
  remarks.append(remark);
  const QJsonObject remarkNotice{{QStringLiteral("level"), QStringLiteral("info")},
                                 {QStringLiteral("message"), remark},
                                 {QStringLiteral("target"), QStringLiteral("F3")},
                                 {QStringLiteral("places"), edges}};
  QJsonArray notices;
  notices.append(remarkNotice);
  QJsonObject fillet{{QStringLiteral("uid"), QStringLiteral("F3")},
                     {QStringLiteral("name"), QStringLiteral("Fillet1")},
                     {QStringLiteral("type"), QStringLiteral("fillet")},
                     {QStringLiteral("status"), QStringLiteral("ok")},
                     {QStringLiteral("level"), QJsonValue()},
                     {QStringLiteral("notices"), notices},
                     {QStringLiteral("remarks"), remarks}};
  QJsonArray features;
  features.append(fillet);
  {
    const TimelineModel model(QJsonObject{{QStringLiteral("features"), features}, {QStringLiteral("marker"), 1}});
    const DocumentSnapshot snapshot = DocumentSnapshot::read(model);
    const DocumentSnapshot::Feature* found = snapshot.feature(QStringLiteral("F3"));
    CHECK(found != nullptr);
    if (found != nullptr) {
      CHECK(!found->noted());
      CHECK(found->error.isEmpty());
      CHECK(found->places.isEmpty());
      CHECK(found->status == QStringLiteral("ok"));
    }
  }
  // With a warning too: the warning's message and places, not the remark's.
  QJsonArray point;
  point.append(QJsonObject{{QStringLiteral("kind"), QStringLiteral("point")},
                           {QStringLiteral("at"), QJsonArray{1.0, 2.0, 3.0}}});
  QJsonArray both;
  both.append(QJsonObject{{QStringLiteral("level"), QStringLiteral("warning")},
                          {QStringLiteral("message"), QStringLiteral("The fillet folds back on itself")},
                          {QStringLiteral("target"), QStringLiteral("F3")},
                          {QStringLiteral("places"), point}});
  both.append(remarkNotice);
  fillet.insert(QStringLiteral("status"), QStringLiteral("warning"));
  fillet.insert(QStringLiteral("level"), QStringLiteral("warning"));
  fillet.insert(QStringLiteral("notices"), both);
  QJsonArray warned;
  warned.append(fillet);
  const TimelineModel model(QJsonObject{{QStringLiteral("features"), warned}, {QStringLiteral("marker"), 1}});
  const DocumentSnapshot snapshot = DocumentSnapshot::read(model);
  const DocumentSnapshot::Feature* found = snapshot.feature(QStringLiteral("F3"));
  CHECK(found != nullptr);
  if (found != nullptr) {
    CHECK(found->error == QStringLiteral("The fillet folds back on itself"));
    CHECK(found->places == point);
  }
}

} // namespace

int documentSnapshotTests() {
  failures = 0;
  readTests();
  handMadeTests();
  convertedSketchTests();
  remarkTests();
  return failures;
}
MITCAD_UNIT_TEST(documentSnapshotTests);
