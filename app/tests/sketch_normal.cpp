// SPDX-License-Identifier: MIT
// A sketch's normal in the design (mitcad#958): the `sketch` query's frame
// is in the sketch's component's coordinates, so a sketch in a component
// whose occurrence is turned a quarter about X extruded along +Z of the
// component, 90° off the profiles the view showed. Extrude's arrow takes
// the normal through the occurrence's `world` placement of the
// `components` query, nested and oblique ones too (a synthetic design).
#include "commands/SketchNormal.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>

#include <gp_Pnt.hxx>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch_normal.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::cmd::occurrenceWorldIn;
using mitcad::cmd::sketchNormalInDesign;

QJsonArray row(double a, double b, double c, double d) {
  QJsonArray values;
  values.append(a);
  values.append(b);
  values.append(c);
  values.append(d);
  return values;
}

QJsonArray rows(const QJsonArray& r0, const QJsonArray& r1, const QJsonArray& r2) {
  QJsonArray matrix;
  matrix.append(r0);
  matrix.append(r1);
  matrix.append(r2);
  matrix.append(row(0, 0, 0, 1));
  return matrix;
}

QJsonArray xyz(double x, double y, double z) {
  QJsonArray values;
  values.append(x);
  values.append(y);
  values.append(z);
  return values;
}

bool along(const std::optional<gp_Dir>& normal, double x, double y, double z) {
  return normal && std::abs(normal->X() - x) < 1e-9 && std::abs(normal->Y() - y) < 1e-9 &&
         std::abs(normal->Z() - z) < 1e-9;
}

// Brace:1 (O1) turned a quarter about X (y to -z, z to y) and raised 100;
// Bracket:1 (O1/O4) inside it at an oblique turn, moved.
QJsonArray occurrences() {
  const double h = std::sqrt(0.5);
  QJsonObject bracket{{QStringLiteral("uid"), QStringLiteral("O4")},
                      {QStringLiteral("component"), QStringLiteral("C2")},
                      {QStringLiteral("world"), rows(row(h, 0, h, 5), row(h, 0, -h, 6), row(0, 1, 0, 107))},
                      {QStringLiteral("children"), QJsonArray()}};
  QJsonArray children;
  children.append(bracket);
  QJsonObject brace{{QStringLiteral("uid"), QStringLiteral("O1")},
                    {QStringLiteral("component"), QStringLiteral("C1")},
                    {QStringLiteral("world"), rows(row(1, 0, 0, 0), row(0, 0, 1, 0), row(0, -1, 0, 100))},
                    {QStringLiteral("children"), children}};
  QJsonArray top;
  top.append(brace);
  return top;
}

void normalTests() {
  const QJsonArray tree = occurrences();
  const QJsonObject frame{{QStringLiteral("origin"), xyz(0, 0, 0)},
                          {QStringLiteral("x_axis"), xyz(1, 0, 0)},
                          {QStringLiteral("y_axis"), xyz(0, 1, 0)},
                          {QStringLiteral("normal"), xyz(0, 0, 1)}};
  // In the root the frame is the design's.
  CHECK(along(sketchNormalInDesign(frame, tree, QString()), 0, 0, 1));
  // In the turned occurrence the arrow points along the world normal, +Y,
  // not the component's +Z.
  CHECK(along(sketchNormalInDesign(frame, tree, QStringLiteral("O1")), 0, 1, 0));
  // Nested and oblique: the nested occurrence's own world placement.
  const double h = std::sqrt(0.5);
  CHECK(along(sketchNormalInDesign(frame, tree, QStringLiteral("O1/O4")), h, -h, 0));
  // A path that leads nowhere keeps the component's normal; no normal, none.
  CHECK(along(sketchNormalInDesign(frame, tree, QStringLiteral("O9")), 0, 0, 1));
  CHECK(!sketchNormalInDesign(QJsonObject(), tree, QStringLiteral("O1")));
}

void worldTests() {
  const QJsonArray tree = occurrences();
  // The root's empty path: identity.
  const std::optional<gp_Trsf> root = occurrenceWorldIn(tree, QString());
  CHECK(root && root->TranslationPart().Modulus() < 1e-12);
  // The placement moves the component's points too (the profiles' middle
  // of a profile that names no occurrence).
  const std::optional<gp_Trsf> brace = occurrenceWorldIn(tree, QStringLiteral("O1"));
  CHECK(brace.has_value());
  if (brace) {
    const gp_Pnt moved = gp_Pnt(1, 2, 3).Transformed(*brace);
    CHECK(moved.Distance(gp_Pnt(1, 3, 98)) < 1e-9);
  }
  CHECK(occurrenceWorldIn(tree, QStringLiteral("O1/O4")).has_value());
  CHECK(!occurrenceWorldIn(tree, QStringLiteral("O4")).has_value()); // not from the root
  CHECK(!occurrenceWorldIn(tree, QStringLiteral("O1/O9")).has_value());
}

} // namespace

int sketchNormalTests() {
  failures = 0;
  normalTests();
  worldTests();
  if (failures == 0) {
    std::printf("sketch normal tests passed\n");
  }
  return failures;
}
