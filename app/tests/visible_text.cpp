// SPDX-License-Identifier: MIT
// Text a person sees (mitcad#1741): the UI tests' visible-text guard finds
// topological names, region keys and bare internal ids, not function keys
// or ordinary words; its allow-list; and the selection card's words for a
// selected profile, sketch curve, constraint and dimension, without the
// region key or an item's id.
#include "framework/Selection.hpp"
#include "framework/VisibleTextGuard.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "visible_text.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QStringList ids(const char* text) { return mitcad::visible_text::identifiersIn(QString::fromUtf8(text)); }

} // namespace

int visibleTextTests() {
  using namespace mitcad;
  failures = 0;

  // Found: topological names, region keys, bare ids as whole words.
  CHECK(ids("Selected: 1 profile r{b{E{F2:end(r{c1[c4,c2]})}}}").contains(QStringLiteral("r{")));
  CHECK(ids("Face F2:end(r{c1}) of F2.b0").contains(QStringLiteral("F2:end")));
  CHECK(ids("Face F2:end(r{c1}) of F2.b0").contains(QStringLiteral("F2")));
  CHECK(ids("Edge E{F1:top|F2:side(c1)}").contains(QStringLiteral("E{")));
  CHECK(ids("Vertex V{a|b|c}").contains(QStringLiteral("V{")));
  CHECK(ids("Body: F9.b0") == QStringList{QStringLiteral("F9")});
  CHECK(ids("Point p15 of Sketch1") == QStringList{QStringLiteral("p15")});
  // A version's short commit hash is not an id even when it reads as one.
  CHECK(ids("other.mitcad v1 Version 1 (c553889), saved by Ada").isEmpty());
  CHECK(ids("Constraint k12, curve c4") == (QStringList{QStringLiteral("k12"), QStringLiteral("c4")}));
  CHECK(ids("in O1/O3") == (QStringList{QStringLiteral("O1"), QStringLiteral("O3")}));
  CHECK(ids("Extrude1 (F57)") == QStringList{QStringLiteral("F57")});
  CHECK(ids("F3") == QStringList());   // a key alone ("F3" is also a key)
  CHECK(ids("F42").size() == 1);       // no key
  // Not ids: names and words that hold such letters, function keys, sizes.
  CHECK(ids("Sketch1, Body12, Extrude3: Profile of Sketch3").isEmpty());
  CHECK(ids("Fit (F6)").isEmpty());
  CHECK(ids("Rename: Ctrl+F2, Shift+F10").isEmpty());
  CHECK(ids("Press F2 to rename the feature").isEmpty());
  CHECK(ids("F2 (reserved)").isEmpty());
  CHECK(ids("F4 + Left drag").isEmpty());
  CHECK(ids("F3 is used by both Fit Point Spline and Command Search in a sketch. Change one of them.").isEmpty());
  CHECK(ids("the F1 key opens the user guide").isEmpty());
  CHECK(ids("M8 screw, H7/g6, A4 paper, #c4c4c4, 0.60.0").isEmpty());
  CHECK(ids("Area: 1200 mm², Perimeter: 140 mm").isEmpty());

  // Rich text as a person reads it.
  CHECK(visible_text::plainText(QStringLiteral("<b>Face</b><br>Body: A &amp; B &lt;M8&gt;")) ==
        QStringLiteral("Face\nBody: A & B <M8>"));
  CHECK(visible_text::plainText(QStringLiteral("Plain <text> stays")) == QStringLiteral("Plain <text> stays"));
  CHECK(visible_text::plainText(QStringLiteral("<p>x&#160;y&#x41;</p>")) ==
        QString(QStringLiteral("x") + QChar(160) + QStringLiteral("yA\n")));

  // The allow-list: each entry a token, a text and a reason.
  QStringList errors;
  const auto entries = visible_text::parseAllowList(
      QStringLiteral("# comment\n\n"
                     "p[12] | ^Component p\\d$ | the test names its components p1 and p2\n"
                     "k1 | x\n"
                     "c1 | ( | broken regex\n"),
      &errors);
  CHECK(entries.size() == 1);
  CHECK(errors.size() == 2);
  CHECK(visible_text::allowed(entries, QStringLiteral("p1"), QStringLiteral("Component p1")));
  CHECK(!visible_text::allowed(entries, QStringLiteral("p1"), QStringLiteral("Point p1 of Sketch1")));
  CHECK(!visible_text::allowed(entries, QStringLiteral("c1"), QStringLiteral("Component p1")));
  CHECK(visible_text::allowed(entries, QStringLiteral("p2"), QStringLiteral("Component p2")));
  CHECK(!visible_text::allowed(entries, QStringLiteral("p12"), QStringLiteral("Component p12")));

  // The selection card's words for sketch items, from the sketch query.
  const QString key = QStringLiteral("r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}");
  const QJsonObject sketch{
      {QStringLiteral("entities"),
       QJsonArray{QJsonObject{{QStringLiteral("id"), QStringLiteral("c1")},
                              {QStringLiteral("type"), QStringLiteral("line")},
                              {QStringLiteral("geometry"),
                               QJsonObject{{QStringLiteral("start"), QJsonArray{0, 0}},
                                           {QStringLiteral("end"), QJsonArray{30, 40}}}}},
                  QJsonObject{{QStringLiteral("id"), QStringLiteral("c5")},
                              {QStringLiteral("type"), QStringLiteral("arc")},
                              {QStringLiteral("geometry"),
                               QJsonObject{{QStringLiteral("radius"), 5.0},
                                           {QStringLiteral("start_angle"), 0.0},
                                           {QStringLiteral("end_angle"), 3.14159265358979}}}}}},
      {QStringLiteral("constraints"),
       QJsonArray{QJsonObject{{QStringLiteral("id"), QStringLiteral("k6")},
                              {QStringLiteral("type"), QStringLiteral("horizontal")}}}},
      {QStringLiteral("dimensions"),
       QJsonArray{QJsonObject{{QStringLiteral("id"), QStringLiteral("k7")},
                              {QStringLiteral("type"), QStringLiteral("length")},
                              {QStringLiteral("value"), QStringLiteral("d1")},
                              {QStringLiteral("measured"), 50.0}}}},
      {QStringLiteral("regions"), QJsonArray{QJsonObject{{QStringLiteral("key"), key},
                                                         {QStringLiteral("area"), 1200.0},
                                                         {QStringLiteral("perimeter"), 140.0}}}}};
  const auto name = [](const QString& type) {
    return type == QStringLiteral("horizontal") ? QStringLiteral("Horizontal") : QString();
  };
  const auto card = [&](SelectKind kind, const QString& item) {
    return sketchItemCard(SelectionItem{kind, QStringLiteral("F3"), item, QString()}, sketch,
                          QStringLiteral("Sketch3"), name);
  };
  const auto clean = [](const SketchItemCard& c) {
    return visible_text::identifiersIn(c.title).isEmpty() &&
           visible_text::identifiersIn(visible_text::plainText(c.measurements)).isEmpty();
  };
  SketchItemCard shown = card(SelectKind::Profile, key);
  CHECK(shown.title == QStringLiteral("Profile of Sketch3"));
  CHECK(shown.measurements.contains(QStringLiteral("Area: <b>1200 mm²")));
  CHECK(shown.measurements.contains(QStringLiteral("Perimeter: <b>140 mm")));
  CHECK(clean(shown));
  shown = card(SelectKind::SketchCurve, QStringLiteral("c1"));
  CHECK(shown.title == QStringLiteral("Line of Sketch3"));
  CHECK(shown.measurements.contains(QStringLiteral("Length: <b>50 mm")));
  CHECK(clean(shown));
  shown = card(SelectKind::SketchCurve, QStringLiteral("c5"));
  CHECK(shown.title == QStringLiteral("Arc of Sketch3"));
  CHECK(shown.measurements.contains(QStringLiteral("Radius: <b>5 mm")));
  CHECK(shown.measurements.contains(QStringLiteral("Length: <b>15.708 mm")));
  CHECK(clean(shown));
  shown = card(SelectKind::SketchPoint, QStringLiteral("p2"));
  CHECK(shown.title == QStringLiteral("Point of Sketch3"));
  CHECK(shown.measurements.isEmpty());
  shown = card(SelectKind::SketchConstraint, QStringLiteral("k6"));
  CHECK(shown.title == QStringLiteral("Horizontal constraint in Sketch3"));
  CHECK(clean(shown));
  shown = card(SelectKind::SketchConstraint, QStringLiteral("fix:p2"));
  CHECK(shown.title == QStringLiteral("Fix constraint in Sketch3"));
  shown = card(SelectKind::SketchDimension, QStringLiteral("k7"));
  CHECK(shown.title == QStringLiteral("Dimension d1 in Sketch3"));
  CHECK(shown.measurements.contains(QStringLiteral("Value: <b>50 mm")));
  CHECK(clean(shown));
  // A region the answer does not have (a key from before an edit) and no
  // answer at all: named, not measured, and never by its key.
  shown = sketchItemCard(SelectionItem{SelectKind::Profile, QStringLiteral("F3"), QStringLiteral("r{c9}"), QString()},
                         QJsonObject(), QString(), name);
  CHECK(shown.title == QStringLiteral("Profile"));
  CHECK(shown.measurements.isEmpty());
  return failures;
}
