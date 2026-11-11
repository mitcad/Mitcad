// SPDX-License-Identifier: MIT
// The document snapshot's feature lookup (mitcad#833): a feature found by
// its uid through the index read() makes, as the browser and the view look
// one up per row, body and profile of a design with thousands of features;
// a missing uid; and a snapshot whose features were added otherwise.
#include "browser/DocumentSnapshot.hpp"

#include <cstdio>
#include <memory>

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

} // namespace

int documentSnapshotTests() {
  failures = 0;
  readTests();
  handMadeTests();
  convertedSketchTests();
  return failures;
}
