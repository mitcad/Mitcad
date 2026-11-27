// SPDX-License-Identifier: MIT
// Minimum Distance (mitcad#2519): a selection's items as the analysis'
// picks and back, the definition with and without a clearance, the query
// it asks and its error in words, the answer in the log's line and in the
// result area's words, and the line between the closest points the view
// draws with its value and legend, in the distance's color or the
// clearance's.
#include "UnitTests.hpp"
#include "commands/MinimumDistance.hpp"
#include "commands/PrintAnalysis.hpp"

#include <array>
#include <cmath>
#include <cstdio>
#include <optional>

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "minimum_distance.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool same(double a, double b) { return std::abs(a - b) < 1e-9; }

mitcad::SelectionItem item(mitcad::SelectKind kind, const QString& owner, const QString& name,
                           const QString& occurrence) {
  mitcad::SelectionItem result{kind, owner, name, QString()};
  result.occurrence = occurrence;
  return result;
}

QJsonArray point(double x, double y, double z) {
  QJsonArray p;
  p.append(x);
  p.append(y);
  p.append(z);
  return p;
}

// The model's answer for two 10 mm cubes 5 mm apart along Z
// (core/model-api's minimum_distance), with a clearance of 6 mm.
QJsonObject apartAnswer(bool withClearance) {
  QJsonArray points;
  points.append(point(5.0, 5.0, 10.0));
  points.append(point(5.0, 5.0, 15.0));
  QJsonObject answer{
      {QStringLiteral("distance"), 5.0},
      {QStringLiteral("points"), points},
      {QStringLiteral("touching"), false},
      {QStringLiteral("inside"), false},
      {QStringLiteral("first"), QJsonObject{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                                            {QStringLiteral("name"), QStringLiteral("Body1")},
                                            {QStringLiteral("pick"), 0}}},
      {QStringLiteral("second"), QJsonObject{{QStringLiteral("body"), QStringLiteral("F4.b0")},
                                             {QStringLiteral("occurrence"), QStringLiteral("O1")},
                                             {QStringLiteral("name"), QStringLiteral("Body2")},
                                             {QStringLiteral("pick"), 0}}},
      {QStringLiteral("pairs"), 2},
      {QStringLiteral("measured"), 1}};
  if (withClearance) {
    answer.insert(QStringLiteral("clearance"), 6.0);
    answer.insert(QStringLiteral("within_clearance"), true);
  }
  return answer;
}

void pickTests() {
  using mitcad::SelectKind;
  const mitcad::Selection items = {
      item(SelectKind::Body, QStringLiteral("F2.b0"), QString(), QString()),
      item(SelectKind::Body, QStringLiteral("F4.b0"), QString(), QStringLiteral("O1")),
      item(SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("F2:top"), QString()),
      item(SelectKind::Edge, QStringLiteral("F2.b0"), QStringLiteral("E{a|b}"), QString()),
      item(SelectKind::Component, QStringLiteral("C1"), QString(), QStringLiteral("O1")),
      // A repeat and a kind it does not take are left out.
      item(SelectKind::Body, QStringLiteral("F2.b0"), QString(), QString()),
      item(SelectKind::Plane, QStringLiteral("xy"), QString(), QString()),
  };
  const QJsonArray picks = mitcad::distancePicks(items);
  CHECK(picks.size() == 5);
  CHECK(picks.at(0).toObject() == QJsonObject({{QStringLiteral("body"), QStringLiteral("F2.b0")}}));
  CHECK(picks.at(1).toObject() == QJsonObject({{QStringLiteral("body"), QStringLiteral("F4.b0")},
                                               {QStringLiteral("occurrence"), QStringLiteral("O1")}}));
  CHECK(picks.at(2).toObject() == QJsonObject({{QStringLiteral("body"), QStringLiteral("F2.b0")},
                                               {QStringLiteral("face"), QStringLiteral("F2:top")}}));
  CHECK(picks.at(3).toObject().value(QStringLiteral("edge")).toString() == QStringLiteral("E{a|b}"));
  CHECK(picks.at(4).toObject() == QJsonObject({{QStringLiteral("occurrence"), QStringLiteral("O1")}}));

  // Back as the panel's items: a face with its body and occurrence; a
  // component's pick is the command's to make.
  const mitcad::SelectionItem face = mitcad::distancePickItem(QJsonObject{
      {QStringLiteral("body"), QStringLiteral("F4.b0")},
      {QStringLiteral("face"), QStringLiteral("F4:top")},
      {QStringLiteral("occurrence"), QStringLiteral("O1")}});
  CHECK(face.kind == SelectKind::Face && face.owner == QStringLiteral("F4.b0") &&
        face.name == QStringLiteral("F4:top") && face.occurrence == QStringLiteral("O1"));
  const mitcad::SelectionItem body =
      mitcad::distancePickItem(QJsonObject{{QStringLiteral("body"), QStringLiteral("F2.b0")}});
  CHECK(body.kind == SelectKind::Body && body.name.isEmpty() && body.occurrence.isEmpty());
  CHECK(!mitcad::distancePickItem(QJsonObject{{QStringLiteral("occurrence"), QStringLiteral("O1")}}).isValid());
}

void definitionTests() {
  // Appended, not braced (MSVC, as in draft_analysis.cpp).
  QJsonArray first;
  first.append(QJsonObject{{QStringLiteral("body"), QStringLiteral("F2.b0")}});
  QJsonArray second;
  second.append(QJsonObject{{QStringLiteral("occurrence"), QStringLiteral("O1")}});
  const QJsonObject plain = mitcad::minimumDistanceDef(first, second, std::nullopt);
  CHECK(plain.value(QStringLiteral("type")).toString() == QStringLiteral("minimum_distance"));
  CHECK(plain.value(QStringLiteral("first")).toArray() == first);
  CHECK(plain.value(QStringLiteral("second")).toArray() == second);
  CHECK(!plain.contains(QStringLiteral("clearance")));
  CHECK(!mitcad::clearanceOf(plain).has_value());
  // A clearance of zero or less is none.
  CHECK(!mitcad::minimumDistanceDef(first, second, 0.0).contains(QStringLiteral("clearance")));
  const QJsonObject checked = mitcad::minimumDistanceDef(first, second, 6.0);
  CHECK(mitcad::clearanceOf(checked).has_value() && same(*mitcad::clearanceOf(checked), 6.0));

  const QJsonObject request = mitcad::minimumDistanceQuery(checked);
  CHECK(request.value(QStringLiteral("query")).toString() == QStringLiteral("minimum_distance"));
  CHECK(request.value(QStringLiteral("first")).toArray() == first);
  CHECK(request.value(QStringLiteral("second")).toArray() == second);
  CHECK(same(request.value(QStringLiteral("clearance")).toDouble(), 6.0));
  CHECK(!request.contains(QStringLiteral("type")));
  CHECK(!mitcad::minimumDistanceQuery(plain).contains(QStringLiteral("clearance")));

  // The query runs slow; its error comes back in words.
  bool slow = false;
  QJsonObject answer;
  QString error = mitcad::runMinimumDistance(
      checked,
      [&slow](const QJsonObject& asked, bool isSlow) {
        slow = isSlow;
        return asked.value(QStringLiteral("query")).toString() == QStringLiteral("minimum_distance")
                   ? apartAnswer(true)
                   : QJsonObject();
      },
      answer);
  CHECK(slow);
  CHECK(error.isEmpty());
  CHECK(same(answer.value(QStringLiteral("distance")).toDouble(), 5.0));
  error = mitcad::runMinimumDistance(
      checked,
      [](const QJsonObject&, bool) {
        return QJsonObject{{QStringLiteral("error"), QStringLiteral("the two selections are the same")}};
      },
      answer);
  CHECK(!error.isEmpty());
}

void answerTests() {
  CHECK(mitcad::minimumDistanceLog(apartAnswer(true)) ==
        QStringLiteral("5.000 mm from Body1 (5, 5, 10) to Body2 (5, 5, 15); clearance 6 mm: closer"));
  CHECK(mitcad::minimumDistanceLog(apartAnswer(false)) ==
        QStringLiteral("5.000 mm from Body1 (5, 5, 10) to Body2 (5, 5, 15)"));
  QJsonObject clear = apartAnswer(true);
  clear.insert(QStringLiteral("clearance"), 4.0);
  clear.insert(QStringLiteral("within_clearance"), false);
  CHECK(mitcad::minimumDistanceLog(clear).endsWith(QStringLiteral("; clearance 4 mm: clear")));
  QJsonObject touching = apartAnswer(false);
  touching.insert(QStringLiteral("distance"), 0.0);
  touching.insert(QStringLiteral("touching"), true);
  CHECK(mitcad::minimumDistanceLog(touching).startsWith(QStringLiteral("0.000 mm (touching) from Body1")));

  const std::array<QColor, 2> colors = {QColor(0x00, 0xa6, 0xb4), QColor(0xd0, 0x20, 0x20)};
  const auto length = [](double millimetres) { return QStringLiteral("%1 mm").arg(millimetres); };
  const QString text = mitcad::minimumDistanceText(apartAnswer(true), length, colors);
  CHECK(text.contains(QStringLiteral("Minimum distance: 5 mm")));
  CHECK(text.contains(QStringLiteral("From Body1 at (5, 5, 10)")));
  CHECK(text.contains(QStringLiteral("To Body2 at (5, 5, 15)")));
  CHECK(text.contains(QStringLiteral("color:#d02020")));
  CHECK(text.contains(QStringLiteral("Closer than the clearance of 6 mm")));
  const QString clearText = mitcad::minimumDistanceText(clear, length, colors);
  CHECK(clearText.contains(QStringLiteral("color:#00a6b4")));
  CHECK(clearText.contains(QStringLiteral("Clear of the clearance of 4 mm")));
  CHECK(mitcad::minimumDistanceText(touching, length, colors).contains(QStringLiteral("The selections touch.")));
}

void overlayTests() {
  const QJsonObject json = mitcad::minimumDistanceOverlayJson(apartAnswer(true), QStringLiteral("MinDistance1"));
  CHECK(json.value(QStringLiteral("title")).toString() == QStringLiteral("MinDistance1"));
  const QJsonObject distance = json.value(QStringLiteral("minimum_distance")).toObject();
  CHECK(distance.value(QStringLiteral("points")).toArray().size() == 6);
  CHECK(distance.value(QStringLiteral("within")).toBool());
  CHECK(same(distance.value(QStringLiteral("clearance")).toDouble(), 6.0));

  const QColor overhang(0x7e, 0x3f, 0xd0);
  const QColor thin(0xc2, 0x18, 0x8b);
  const QColor gap(0x00, 0x89, 0x7b);
  const std::array<QColor, 2> colors = {QColor(0x00, 0xa6, 0xb4), QColor(0xd0, 0x20, 0x20)};
  const mitcad::PrintOverlay overlay =
      mitcad::printOverlayOf(json, overhang, thin, gap, {}, {}, QColor(), colors);
  CHECK(!overlay.isEmpty());
  CHECK(overlay.distanceLine && overlay.closerThanClearance);
  CHECK(overlay.overhangs.empty() && overlay.thin.empty() && !overlay.thinnest);
  CHECK(overlay.gaps.size() == 6);
  CHECK(same(overlay.gaps[2], 10.0) && same(overlay.gaps[5], 15.0));
  CHECK(overlay.gapColor == colors[1]);
  CHECK(overlay.narrowest.has_value() && same((*overlay.narrowest)[2], 12.5));
  CHECK(overlay.narrowestLabel == QStringLiteral("5.00 mm"));
  CHECK(overlay.legend.size() == 1);
  CHECK(overlay.legend[0].first == colors[1] &&
        overlay.legend[0].second == QStringLiteral("Closer than the clearance, 6 mm"));
  CHECK(mitcad::printOverlayLog(overlay) ==
        QStringLiteral("Print overlay: 0 overhang triangle(s) #7e3fd0, 0 thin triangle(s) #c2188b (0.0 mm^2), "
                       "no marker, minimum distance #d02020 closer than the clearance, marker 5.00 mm at "
                       "(5, 5, 12.5); legend MinDistance1: #d02020 Closer than the clearance, 6 mm"));

  // Clear of a clearance: the distance's color; without one, its plain
  // words.
  QJsonObject clear = apartAnswer(true);
  clear.insert(QStringLiteral("clearance"), 4.0);
  clear.insert(QStringLiteral("within_clearance"), false);
  const mitcad::PrintOverlay clearOverlay = mitcad::printOverlayOf(
      mitcad::minimumDistanceOverlayJson(clear, QStringLiteral("MinDistance1")), overhang, thin, gap, {}, {}, QColor(),
      colors);
  CHECK(!clearOverlay.closerThanClearance && clearOverlay.gapColor == colors[0]);
  CHECK(clearOverlay.legend.size() == 1 &&
        clearOverlay.legend[0].second == QStringLiteral("Minimum distance, clear of 4 mm"));
  const mitcad::PrintOverlay plainOverlay = mitcad::printOverlayOf(
      mitcad::minimumDistanceOverlayJson(apartAnswer(false), QStringLiteral("Minimum Distance")), overhang, thin, gap,
      {}, {}, QColor(), colors);
  CHECK(plainOverlay.legend.size() == 1 && plainOverlay.legend[0].second == QStringLiteral("Minimum distance"));
  CHECK(mitcad::printOverlayLog(plainOverlay).contains(QStringLiteral(", minimum distance #00a6b4, marker 5.00 mm")));
  // A Print Analysis' narrow gaps stay narrow gaps.
  CHECK(!mitcad::printOverlayOf(QJsonObject(), overhang, thin, gap).distanceLine);
}

} // namespace

int minimumDistanceTests() {
  failures = 0;
  pickTests();
  definitionTests();
  answerTests();
  overlayTests();
  return failures;
}
MITCAD_UNIT_TEST(minimumDistanceTests);
