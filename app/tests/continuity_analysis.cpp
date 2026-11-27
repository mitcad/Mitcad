// SPDX-License-Identifier: MIT
// Continuity Analysis (mitcad#2521): the query's bodies from the picks
// (edges and faces per body and occurrence, a body picked whole for all its
// edges, other kinds left out), the query with its settings, the answer in
// the log's line and the result area's words with the swatches and the
// worst edges first, and the edges the view draws by their classes, a
// closed edge as a closed polyline.
#include "UnitTests.hpp"
#include "commands/ContinuityAnalysis.hpp"

#include <array>
#include <cstdio>
#include <vector>

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>

#include <BRep_Tool.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Wire.hxx>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "continuity_analysis.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

mitcad::SelectionItem pick(mitcad::SelectKind kind, const QString& owner, const QString& name,
                           const QString& occurrence = QString()) {
  mitcad::SelectionItem item;
  item.kind = kind;
  item.owner = owner;
  item.name = name;
  item.occurrence = occurrence;
  return item;
}

// A point as the answer gives it. Appended, not braced: MSVC reads a braced
// list whose one element is a QJsonArray as a copy of that array.
QJsonArray point(double x, double y, double z) {
  QJsonArray p;
  p.append(x);
  p.append(y);
  p.append(z);
  return p;
}

QJsonArray points(const std::vector<std::array<double, 3>>& at) {
  QJsonArray all;
  for (const auto& p : at) {
    all.append(point(p[0], p[1], p[2]));
  }
  return all;
}

int edgeCount(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer it(shape, TopAbs_EDGE); it.More(); it.Next()) {
    ++count;
  }
  return count;
}

void bodiesTests() {
  using mitcad::SelectKind;
  const mitcad::Selection picks = {
      pick(SelectKind::Edge, QStringLiteral("F2.b0"), QStringLiteral("E1")),
      pick(SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("F2:top")),
      pick(SelectKind::Edge, QStringLiteral("F2.b0"), QStringLiteral("E1")),
      pick(SelectKind::Face, QStringLiteral("F3.b0"), QStringLiteral("F3:side")),
      pick(SelectKind::Body, QStringLiteral("F3.b0"), QString()),
      pick(SelectKind::Edge, QStringLiteral("F2.b0"), QStringLiteral("E2"), QStringLiteral("O1")),
      pick(SelectKind::Vertex, QStringLiteral("F2.b0"), QStringLiteral("V1")),
      pick(SelectKind::Edge, QStringLiteral("F2.b0"), QString()),
  };
  const QJsonArray bodies = mitcad::continuityBodies(picks);
  CHECK(bodies.size() == 3);
  const QJsonObject first = bodies.at(0).toObject();
  CHECK(first.value(QStringLiteral("body")).toString() == QStringLiteral("F2.b0"));
  CHECK(!first.contains(QStringLiteral("occurrence")));
  CHECK(first.value(QStringLiteral("edges")).toArray().size() == 1);
  CHECK(first.value(QStringLiteral("edges")).toArray().at(0).toString() == QStringLiteral("E1"));
  CHECK(first.value(QStringLiteral("faces")).toArray().size() == 1);
  CHECK(first.value(QStringLiteral("faces")).toArray().at(0).toString() == QStringLiteral("F2:top"));
  // A body picked whole: every edge of it, its face picked beside it left
  // out.
  const QJsonObject whole = bodies.at(1).toObject();
  CHECK(whole.value(QStringLiteral("body")).toString() == QStringLiteral("F3.b0"));
  CHECK(!whole.contains(QStringLiteral("edges")) && !whole.contains(QStringLiteral("faces")));
  // The same body in an occurrence is a body of its own.
  const QJsonObject placed = bodies.at(2).toObject();
  CHECK(placed.value(QStringLiteral("occurrence")).toString() == QStringLiteral("O1"));
  CHECK(placed.value(QStringLiteral("edges")).toArray().at(0).toString() == QStringLiteral("E2"));
  CHECK(mitcad::continuityBodies(mitcad::Selection()).isEmpty());

  const QJsonObject query = mitcad::continuityQuery(bodies, 16, 0.01, 0.1, 2.0);
  CHECK(query.value(QStringLiteral("query")).toString() == QStringLiteral("edge_continuity"));
  CHECK(query.value(QStringLiteral("bodies")).toArray() == bodies);
  CHECK(query.value(QStringLiteral("samples")).toInt() == 16);
  CHECK(query.value(QStringLiteral("gap")).toDouble() == 0.01);
  CHECK(query.value(QStringLiteral("angle")).toDouble() == 0.1);
  CHECK(query.value(QStringLiteral("curvature")).toDouble() == 2.0);

  CHECK(mitcad::continuityRole(QStringLiteral("g2")) == mitcad::ColorRole::ContinuityG2);
  CHECK(mitcad::continuityRole(QStringLiteral("g1")) == mitcad::ColorRole::ContinuityG1);
  CHECK(mitcad::continuityRole(QStringLiteral("g0")) == mitcad::ColorRole::ContinuityG0);
  CHECK(mitcad::continuityRole(QStringLiteral("none")) == mitcad::ColorRole::ContinuityNone);
}

// A box's edge (G0, 90°), a tangent rounding's edge (G1, 100 %) and a free
// edge, not measured.
QJsonObject answer() {
  QJsonObject sharp{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                    {QStringLiteral("name"), QStringLiteral("Body1")},
                    {QStringLiteral("edge"), QStringLiteral("E1")},
                    {QStringLiteral("sides"), QStringLiteral("shared")},
                    {QStringLiteral("class"), QStringLiteral("g0")},
                    {QStringLiteral("samples"), 17},
                    {QStringLiteral("gap"), 0.0},
                    {QStringLiteral("angle"), 90.0},
                    {QStringLiteral("curvature"), 0.0},
                    {QStringLiteral("curvature_percent"), 0.0}};
  sharp.insert(QStringLiteral("points"), points({{0, 0, 0}, {10, 0, 0}}));
  QJsonObject tangent{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                      {QStringLiteral("name"), QStringLiteral("Body1")},
                      {QStringLiteral("edge"), QStringLiteral("E2")},
                      {QStringLiteral("sides"), QStringLiteral("shared")},
                      {QStringLiteral("class"), QStringLiteral("g1")},
                      {QStringLiteral("samples"), 17},
                      {QStringLiteral("gap"), 0.0},
                      {QStringLiteral("angle"), 0.0},
                      {QStringLiteral("curvature"), 0.25},
                      {QStringLiteral("curvature_percent"), 100.0}};
  tangent.insert(QStringLiteral("points"), points({{0, 0, 0}, {0, 5, 1}, {0, 10, 4}}));
  QJsonObject freeEdge{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                       {QStringLiteral("name"), QStringLiteral("Body1")},
                       {QStringLiteral("edge"), QStringLiteral("E3")},
                       {QStringLiteral("sides"), QStringLiteral("open")},
                       {QStringLiteral("class"), QStringLiteral("unmeasured")},
                       {QStringLiteral("samples"), 0}};
  freeEdge.insert(QStringLiteral("points"), points({{0, 0, 0}, {0, 0, 10}}));
  QJsonArray edges;
  edges.append(sharp);
  edges.append(tangent);
  edges.append(freeEdge);
  return {{QStringLiteral("samples"), 16},
          {QStringLiteral("gap"), 0.01},
          {QStringLiteral("angle"), 0.1},
          {QStringLiteral("curvature"), 2.0},
          {QStringLiteral("edges"), edges},
          {QStringLiteral("counts"), QJsonObject{{QStringLiteral("none"), 0},
                                                 {QStringLiteral("g0"), 1},
                                                 {QStringLiteral("g1"), 1},
                                                 {QStringLiteral("g2"), 0},
                                                 {QStringLiteral("unmeasured"), 1}}},
          {QStringLiteral("worst"), QJsonObject{{QStringLiteral("gap"), 0.0},
                                                {QStringLiteral("angle"), 90.0},
                                                {QStringLiteral("curvature_percent"), 100.0}}}};
}

void answerTests() {
  const QJsonObject found = answer();
  CHECK(mitcad::continuityLog(found) ==
        QStringLiteral("3 edges: g2 0, g1 1, g0 1, none 0, unmeasured 1; worst gap 0.0000 mm, angle 90.000 deg, "
                       "curvature 100.0 %"));
  const std::array<QColor, 4> colors = {QColor(QStringLiteral("#00897b")), QColor(QStringLiteral("#2166ac")),
                                        QColor(QStringLiteral("#7b4fd0")), QColor(QStringLiteral("#b5179e"))};
  const QString text = mitcad::continuityText(found, colors);
  CHECK(text.contains(
      QStringLiteral("Within a gap of 0.01 mm, an angle of 0.1° and a curvature difference of 2 %.")));
  CHECK(text.contains(QStringLiteral("<span style='color:#00897b'>&#9632;</span> G2, curvature continuous: 0")));
  CHECK(text.contains(QStringLiteral("<span style='color:#2166ac'>&#9632;</span> G1, tangent: 1")));
  CHECK(text.contains(QStringLiteral("<span style='color:#7b4fd0'>&#9632;</span> G0, touching: 1")));
  CHECK(text.contains(QStringLiteral("<span style='color:#b5179e'>&#9632;</span> Not touching: 0")));
  CHECK(text.contains(QStringLiteral("not measured")));
  CHECK(text.contains(QStringLiteral("Worst: gap 0.0000 mm, angle 90.000°, curvature 100.0 %")));
  // The worst edge first; the free edge is not listed; no topological name.
  const auto sharpAt = text.indexOf(QStringLiteral("Body1, edge 1: G0, gap 0.0000 mm, angle 90.000°, curvature 0.0 %"));
  const auto tangentAt = text.indexOf(QStringLiteral("Body1, edge 2: G1, gap 0.0000 mm, angle 0.000°, curvature 100.0 %"));
  CHECK(sharpAt > 0 && tangentAt > sharpAt);
  CHECK(!text.contains(QStringLiteral("edge 3:")));
  CHECK(!text.contains(QStringLiteral("E1")) && !text.contains(QStringLiteral("E2")));

  // Drawn by class, the best first: the G1 edge's polyline of two
  // segments, the G0 edge's of one; the free edge is not drawn.
  const auto drawn = mitcad::continuityEdges(found);
  CHECK(drawn.size() == 2);
  if (drawn.size() == 2) {
    CHECK(drawn[0].second == mitcad::ColorRole::ContinuityG1);
    CHECK(edgeCount(drawn[0].first) == 2);
    CHECK(drawn[1].second == mitcad::ColorRole::ContinuityG0);
    CHECK(edgeCount(drawn[1].first) == 1);
  }

  // A closed edge (its last point its first) as a closed polyline: four
  // segments round a square, the first point not run back to twice.
  QJsonObject circle{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                     {QStringLiteral("name"), QStringLiteral("Body1")},
                     {QStringLiteral("class"), QStringLiteral("none")},
                     {QStringLiteral("gap"), 0.5},
                     {QStringLiteral("angle"), 0.0},
                     {QStringLiteral("curvature_percent"), 0.0}};
  circle.insert(QStringLiteral("points"), points({{0, 0, 0}, {1, 0, 0}, {1, 1, 0}, {0, 1, 0}, {0, 0, 0}}));
  QJsonArray one;
  one.append(circle);
  const auto loop = mitcad::continuityEdges(QJsonObject{{QStringLiteral("edges"), one}});
  CHECK(loop.size() == 1);
  if (loop.size() == 1) {
    CHECK(loop[0].second == mitcad::ColorRole::ContinuityNone);
    CHECK(edgeCount(loop[0].first) == 4);
    int wires = 0;
    for (TopExp_Explorer it(loop[0].first, TopAbs_WIRE); it.More(); it.Next()) {
      ++wires;
      CHECK(BRep_Tool::IsClosed(TopoDS::Wire(it.Current())));
    }
    CHECK(wires == 1);
  }

  // Nothing measured.
  CHECK(mitcad::continuityLog(QJsonObject()) ==
        QStringLiteral("0 edges: g2 0, g1 0, g0 0, none 0, unmeasured 0; worst gap 0.0000 mm, angle 0.000 deg, "
                       "curvature 0.0 %"));
  CHECK(mitcad::continuityEdges(QJsonObject()).empty());
}

} // namespace

int continuityAnalysisTests() {
  failures = 0;
  bodiesTests();
  answerTests();
  return failures;
}
MITCAD_UNIT_TEST(continuityAnalysisTests);
