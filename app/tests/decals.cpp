// SPDX-License-Identifier: MIT
// Decals (mitcad#2244): the model's entries as the view draws them (hidden
// ones, those whose face is gone and grids that are not whole left out),
// the model's triangles of a decal cut at its face's edges (mitcad#2266),
// the decal a point of a face lies on, and the log line UI tests read.
#include "UnitTests.hpp"
#include "view/Decals.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "decals.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// A decal 10 x 5 mm on the plane z = 20, centred at (10, 8): one quad.
QJsonObject flat(const QString& name) {
  const QJsonArray up{0.0, 0.0, 1.0};
  return QJsonObject{
      {QStringLiteral("name"), name},
      {QStringLiteral("sha256"), QStringLiteral("abc")},
      {QStringLiteral("surface"), QStringLiteral("plane")},
      {QStringLiteral("face"), QJsonObject{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                                           {QStringLiteral("face"), QStringLiteral("F2:end(r{c1})")}}},
      {QStringLiteral("width"), 10.0},
      {QStringLiteral("height"), 5.0},
      {QStringLiteral("angle"), 0.0},
      {QStringLiteral("flip"), false},
      {QStringLiteral("opacity"), 1.0},
      {QStringLiteral("visible"), true},
      {QStringLiteral("pixels"), QJsonArray{200, 100}},
      {QStringLiteral("mesh"),
       QJsonObject{{QStringLiteral("columns"), 1},
                   {QStringLiteral("rows"), 1},
                   {QStringLiteral("points"), QJsonArray{QJsonArray{5.0, 10.5, 20.0}, QJsonArray{15.0, 10.5, 20.0},
                                                         QJsonArray{5.0, 5.5, 20.0}, QJsonArray{15.0, 5.5, 20.0}}},
                   {QStringLiteral("normals"), QJsonArray{up, up, up, up}},
                   {QStringLiteral("uvs"), QJsonArray{QJsonArray{0.0, 0.0}, QJsonArray{1.0, 0.0},
                                                      QJsonArray{0.0, 1.0}, QJsonArray{1.0, 1.0}}}}},
  };
}

void entryTests() {
  const std::optional<mitcad::DecalShown> decal = mitcad::decalOf(flat(QStringLiteral("Decal1")));
  CHECK(decal.has_value());
  if (decal) {
    CHECK(decal->body == QStringLiteral("F2.b0"));
    CHECK(decal->face == QStringLiteral("F2:end(r{c1})"));
    CHECK(decal->columns == 1 && decal->rows == 1);
    CHECK(decal->points.size() == 4u);
    CHECK(decal->at(1, 1) == 3u);
    CHECK(decal->pixelsWide == 200);
  }
  // A face that is gone, a grid that is not whole, a hidden decal.
  QJsonObject gone = flat(QStringLiteral("Gone"));
  gone.remove(QStringLiteral("mesh"));
  gone.insert(QStringLiteral("error"), QStringLiteral("body F2.b0 does not exist at the timeline marker"));
  CHECK(!mitcad::decalOf(gone).has_value());
  QJsonObject broken = flat(QStringLiteral("Broken"));
  QJsonObject mesh = broken.value(QStringLiteral("mesh")).toObject();
  mesh.insert(QStringLiteral("columns"), 2);
  broken.insert(QStringLiteral("mesh"), mesh);
  CHECK(!mitcad::decalOf(broken).has_value());
  QJsonObject hidden = flat(QStringLiteral("Hidden"));
  hidden.insert(QStringLiteral("visible"), false);
  const std::vector<mitcad::DecalShown> shown =
      mitcad::shownDecals(QJsonArray{flat(QStringLiteral("Decal1")), gone, broken, hidden});
  CHECK(shown.size() == 1);
  CHECK(!shown.empty() && shown.front().name == QStringLiteral("Decal1"));
}

void pickTests() {
  const std::vector<mitcad::DecalShown> shown = mitcad::shownDecals(QJsonArray{flat(QStringLiteral("Decal1"))});
  // Its middle, a point just off its plane, a point beside it on the face.
  CHECK(mitcad::decalAt(shown, {10.0, 8.0, 20.0}, 0.2) == QStringLiteral("Decal1"));
  CHECK(mitcad::decalAt(shown, {14.9, 5.6, 20.1}, 0.2) == QStringLiteral("Decal1"));
  CHECK(!mitcad::decalAt(shown, {18.0, 8.0, 20.0}, 0.2).has_value());
  CHECK(!mitcad::decalAt(shown, {10.0, 8.0, 21.0}, 0.2).has_value());
}

// Cut at its face's edge x = 10 (mitcad#2266): the model's triangles cover
// its left half, through two points after the grid's.
QJsonObject halfOn(const QString& name) {
  QJsonObject entry = flat(name);
  QJsonObject mesh = entry.value(QStringLiteral("mesh")).toObject();
  QJsonArray points = mesh.value(QStringLiteral("points")).toArray();
  QJsonArray normals = mesh.value(QStringLiteral("normals")).toArray();
  QJsonArray uvs = mesh.value(QStringLiteral("uvs")).toArray();
  points.append(QJsonArray{10.0, 10.5, 20.0});
  points.append(QJsonArray{10.0, 5.5, 20.0});
  normals.append(QJsonArray{0.0, 0.0, 1.0});
  normals.append(QJsonArray{0.0, 0.0, 1.0});
  uvs.append(QJsonArray{0.5, 0.0});
  uvs.append(QJsonArray{0.5, 1.0});
  mesh.insert(QStringLiteral("points"), points);
  mesh.insert(QStringLiteral("normals"), normals);
  mesh.insert(QStringLiteral("uvs"), uvs);
  mesh.insert(QStringLiteral("triangles"), QJsonArray{QJsonArray{0, 2, 5}, QJsonArray{0, 5, 4}});
  mesh.insert(QStringLiteral("cut"), true);
  entry.insert(QStringLiteral("mesh"), mesh);
  return entry;
}

void cutTests() {
  // A decal without triangles is drawn whole: its grid's two.
  const std::optional<mitcad::DecalShown> whole = mitcad::decalOf(flat(QStringLiteral("Decal1")));
  CHECK(whole && whole->triangles.size() == 2u && !whole->cut);
  const std::optional<mitcad::DecalShown> half = mitcad::decalOf(halfOn(QStringLiteral("Decal1")));
  CHECK(half.has_value());
  if (half) {
    CHECK(half->points.size() == 6u && half->triangles.size() == 2u && half->cut);
    CHECK(half->triangles.front()[2] == 5u);
  }
  // Picked only where it is drawn.
  const std::vector<mitcad::DecalShown> shown = mitcad::shownDecals(QJsonArray{halfOn(QStringLiteral("Decal1"))});
  CHECK(mitcad::decalAt(shown, {7.0, 8.0, 20.0}, 0.2) == QStringLiteral("Decal1"));
  CHECK(!mitcad::decalAt(shown, {13.0, 8.0, 20.0}, 0.2).has_value());
  CHECK(mitcad::decalsLog(shown) == QStringLiteral("Decals drawn: Decal1 10.0 x 5.0 mm on a plane 100% cut"));
  // A triangle with a point that is not there.
  QJsonObject broken = halfOn(QStringLiteral("Broken"));
  QJsonObject mesh = broken.value(QStringLiteral("mesh")).toObject();
  mesh.insert(QStringLiteral("triangles"), QJsonArray{QJsonArray{0, 2, 6}});
  broken.insert(QStringLiteral("mesh"), mesh);
  CHECK(!mitcad::decalOf(broken).has_value());
}

void logTests() {
  CHECK(mitcad::decalsLog({}) == QStringLiteral("Decals drawn: none"));
  QJsonObject turned = flat(QStringLiteral("Decal2"));
  turned.insert(QStringLiteral("angle"), 90.0);
  turned.insert(QStringLiteral("flip"), true);
  turned.insert(QStringLiteral("opacity"), 0.5);
  turned.insert(QStringLiteral("surface"), QStringLiteral("cylinder"));
  const std::vector<mitcad::DecalShown> shown =
      mitcad::shownDecals(QJsonArray{flat(QStringLiteral("Decal1")), turned});
  CHECK(mitcad::decalsLog(shown) ==
        QStringLiteral("Decals drawn: Decal1 10.0 x 5.0 mm on a plane 100%, "
                       "Decal2 10.0 x 5.0 mm on a cylinder turned 90 deg flipped 50%"));
  // The panel's live result is a line of its own: the decal drawn the same
  // after OK logs again.
  std::vector<mitcad::DecalShown> panel = mitcad::shownDecals(QJsonArray{flat(QStringLiteral("Decal1"))});
  panel.front().panel = true;
  CHECK(mitcad::decalsLog(panel) == QStringLiteral("Decals drawn: Decal1 10.0 x 5.0 mm on a plane 100% in the panel"));
}

} // namespace

int decalsTests() {
  failures = 0;
  entryTests();
  pickTests();
  cutTests();
  logTests();
  return failures;
}
MITCAD_UNIT_TEST(decalsTests);
