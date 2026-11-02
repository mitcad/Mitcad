// SPDX-License-Identifier: MIT
// Synthetic face-bounded profiles through the real Rust/OCCT bridge.

#include <array>
#include <cmath>
#include <cstdio>
#include <stdexcept>
#include <string>

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QStringList>

#include "mitcad/geometry/query.hpp"
#include "mitcad_bridge/lib.h"

namespace {

constexpr double kPi = 3.14159265358979323846;

void require(bool ok, const char* why) {
  if (!ok) throw std::runtime_error(why);
}

void checkNear(double actual, double expected, const char* why) {
  if (!std::isfinite(actual) || std::abs(actual - expected) > 1e-6) {
    std::fprintf(stderr, "%s: %.15g, expected %.15g (absolute tolerance 1e-6)\n", why, actual, expected);
    throw std::runtime_error(why);
  }
}

QJsonValue json(const rust::String& text) {
  const std::string copy(text);
  QJsonParseError error;
  // QJsonDocument accepts arrays/objects only; bridge queries also return
  // scalar values such as null for a missing profile.
  const auto value = QJsonDocument::fromJson("[" + QByteArray::fromStdString(copy) + "]", &error);
  require(error.error == QJsonParseError::NoError, "bridge returned invalid JSON");
  require(value.isArray() && value.array().size() == 1, "bridge returned no unique JSON value");
  return value.array().first();
}

QJsonValue command(mitcad::Document& doc, const QJsonObject& value) {
  const auto text = QJsonDocument(value).toJson(QJsonDocument::Compact).toStdString();
  return json(doc.command(text));
}

QJsonValue query(const mitcad::Document& doc, const QJsonObject& value) {
  const auto text = QJsonDocument(value).toJson(QJsonDocument::Compact).toStdString();
  return json(doc.query(text));
}

QJsonObject sketch(const mitcad::Document& doc) {
  return query(doc, {{"query", "sketch"}, {"uid", "F3"}}).toObject();
}

QJsonObject extrusion(const QString& sketchUid, const QString& key, const QString& operation, double height) {
  return {{"cmd", "add_feature"}, {"def", QJsonObject{
      {"type", "extrude"}, {"profiles", QJsonArray{QJsonObject{{"sketch", sketchUid}, {"region", key}}}},
      {"extent", QJsonObject{{"type", "distance"}, {"distance", height}}}, {"operation", operation}}}};
}

// The face is selected from the kernel's queried normal, never from an
// assumed edge index or a real design's topological name.
QString circleFace(mitcad::Document& doc) {
  command(doc, {{"cmd", "sketch.create"}});
  command(doc, {{"cmd", "sketch.add_circle"}, {"sketch", "F1"}, {"center", QJsonArray{0, 0}}, {"diameter", 60}});
  const auto regions = query(doc, {{"query", "sketch"}, {"uid", "F1"}}).toObject().value("regions").toArray();
  require(regions.size() == 1, "source circle has no unique profile");
  command(doc, extrusion("F1", regions.first().toObject().value("key").toString(), "new_body", 10));
  const auto faces = query(doc, {{"query", "faces"}, {"body", "F2.b0"}, {"geometry", true},
                                {"normal", QJsonArray{0, 0, 1}}}).toObject().value("faces").toArray();
  require(faces.size() == 1, "cylinder has no unique top planar face");
  const auto face = faces.first().toObject();
  require(face.value("surface").toString() == "plane", "source face is not planar");
  checkNear(face.value("origin").toArray().at(2).toDouble(), 10, "source face height");
  const auto names = face.value("names").toArray();
  require(!names.isEmpty(), "source face has no topological name");
  command(doc, {{"cmd", "set_feature_visible"}, {"uid", "F1"}, {"visible", false}});
  return names.first().toString();
}

void onFace(mitcad::Document& doc, const QString& face) {
  command(doc, {{"cmd", "sketch.create"}, {"plane", QJsonObject{{"body", "F2.b0"}, {"face", face}}}});
  require(sketch(doc).value("entities").toArray().isEmpty(), "face boundary became explicit sketch entities");
}

QJsonObject outside(const mitcad::Document& doc) {
  const auto regions = sketch(doc).value("regions").toArray();
  require(regions.size() == 3, "face, octagon and inward offset must give three regions");
  QJsonObject found;
  int count = 0;
  for (const auto& value : regions) {
    const auto region = value.toObject();
    if (region.value("face_bounded").toBool()) { found = region; ++count; }
  }
  require(count == 1, "expected exactly one face-bounded outside region");
  require(found.value("key").toString().contains("b{E{"), "outside key lost its named face edge");
  return found;
}

QString polygon(mitcad::Document& doc) {
  const auto made = command(doc, {{"cmd", "sketch.polygon"}, {"sketch", "F3"},
      {"center", QJsonArray{0, 0}}, {"vertex", QJsonArray{20, 0}}, {"sides", 8}, {"inscribed", false}})
      .toObject().value("made").toArray();
  require(made.size() == 8, "octagon did not create eight sides");
  command(doc, {{"cmd", "sketch.offset"}, {"sketch", "F3"}, {"curves", made}, {"distance", -2}});
  return outside(doc).value("key").toString();
}

QJsonObject profile(const mitcad::Document& doc, const QString& key) {
  for (const auto& value : query(doc, {{"query", "profiles"}, {"hashes", true}}).toArray()) {
    const auto item = value.toObject();
    if (item.value("sketch").toString() == "F3" && item.value("region").toString() == key) return item;
  }
  throw std::runtime_error("outside region is missing from profiles query");
}

double outerArea(double radius) { return kPi * radius * radius - 8 * 400 * std::tan(kPi / 8); }
double joinedVolume(double radius) { return kPi * radius * radius * 10 + outerArea(radius) * 2; }

void ringTest() {
  auto doc = mitcad::new_document();
  const auto face = circleFace(*doc);
  onFace(*doc, face);
  const auto key = polygon(*doc);
  const auto entityCount = sketch(*doc).value("entities").toArray().size();
  checkNear(outside(*doc).value("area").toDouble(), outerArea(30), "outside region area");
  const auto shape = doc->profile_shape("F3", key.toStdString());
  require(bool(shape), "outside region has no OCCT profile face");
  checkNear(mitcad::geometry::mass_properties(*shape).area, outerArea(30), "OCCT outside profile area");
  const auto hash = profile(*doc, key).value("hash");
  require(profile(*doc, key).value("face_bounded").toBool(), "profiles query lost face_bounded mark");
  command(*doc, extrusion("F3", key, "join", 2));
  require(query(*doc, {{"query", "bodies"}}).toArray().size() == 1, "outside Join did not produce one body");
  auto joined = doc->body_shape("F2.b0");
  require(bool(joined), "Join lost its source body");
  checkNear(mitcad::geometry::volume(*joined), joinedVolume(30), "outside Join volume");

  command(*doc, {{"cmd", "set_parameter"}, {"name", "d1"}, {"value", 80}});
  require(outside(*doc).value("key").toString() == key, "diameter change renamed the outside region");
  require(profile(*doc, key).value("hash") != hash, "diameter change kept stale profile geometry");
  require(sketch(*doc).value("entities").toArray().size() == entityCount, "implicit boundary changed entity count");
  checkNear(outside(*doc).value("area").toDouble(), outerArea(40), "followed outside region area");
  joined = doc->body_shape("F2.b0");
  require(bool(joined), "parameter change lost joined body");
  checkNear(mitcad::geometry::volume(*joined), joinedVolume(40), "followed Join volume");
  command(*doc, {{"cmd", "sketch.redefine_plane"}, {"sketch", "F3"}, {"plane", "xy"}});
  const auto regions = sketch(*doc).value("regions").toArray();
  require(regions.size() == 2, "off-face sketch did not restore its two own regions");
  for (const auto& region : regions) require(!region.toObject().value("face_bounded").toBool(), "stale face boundary");
  const auto lost = query(*doc, {{"query", "feature"}, {"uid", "F4"}}).toObject();
  require(lost.value("status").toString() == "error" && lost.value("error").toString().contains("has no profile"),
          "used outside region rebound after Redefine Sketch Plane");
}

void crossingTest() {
  auto doc = mitcad::new_document();
  onFace(*doc, circleFace(*doc));
  command(*doc, {{"cmd", "sketch.add_line"}, {"sketch", "F3"},
                 {"start", QJsonArray{-40, 0}}, {"end", QJsonArray{40, 0}}});
  const auto regions = sketch(*doc).value("regions").toArray();
  require(regions.size() == 2, "line across face did not split it into two regions");
  for (const auto& value : regions) {
    const auto region = value.toObject();
    require(region.value("face_bounded").toBool(), "half-disc is not marked face_bounded");
    checkNear(region.value("area").toDouble(), kPi * 450, "half-disc area");
    const auto key = region.value("key").toString().toStdString();
    const auto shape = doc->profile_shape("F3", key);
    require(bool(shape), "half-disc has no OCCT profile face");
    checkNear(mitcad::geometry::mass_properties(*shape).area, kPi * 450, "OCCT half-disc area");
  }
  command(*doc, {{"cmd", "sketch.redefine_plane"}, {"sketch", "F3"}, {"offset", 2}});
  require(sketch(*doc).value("regions").toArray().isEmpty(), "offset sketch kept stale face-bounded regions");
}

QString patchFace(mitcad::Document& doc) {
  command(doc, {{"cmd", "sketch.create"}});
  command(doc, {{"cmd", "sketch.add_rectangle"}, {"sketch", "F1"},
                {"corner", QJsonArray{-5, -5}}, {"width", 10}, {"height", 10}});
  const auto source = query(doc, {{"query", "sketch"}, {"uid", "F1"}}).toObject();
  const auto regions = source.value("regions").toArray();
  require(regions.size() == 1 && !regions.first().toObject().value("face_bounded").toBool(),
          "ordinary XY rectangle profile changed");
  const auto key = regions.first().toObject().value("key").toString();
  command(doc, {{"cmd", "add_feature"}, {"def", QJsonObject{{"type", "patch"},
      {"profiles", QJsonArray{QJsonObject{{"sketch", "F1"}, {"region", key}}}}}}});
  const auto shape = doc.body_shape("F2.b0");
  require(bool(shape), "Patch did not create its open surface body");
  checkNear(mitcad::geometry::mass_properties(*shape).area, 100, "source Patch area");
  const auto faces = query(doc, {{"query", "faces"}, {"body", "F2.b0"}, {"geometry", true}})
                         .toArray();
  require(faces.size() == 1, "Patch has no unique planar face");
  const auto face = faces.first().toObject();
  require(face.value("surface").toString() == "plane", "Patch is not planar");
  const auto names = face.value("names").toArray();
  require(!names.isEmpty(), "Patch has no stable face name");
  command(doc, {{"cmd", "set_feature_visible"}, {"uid", "F1"}, {"visible", false}});
  return names.first().toString();
}

// Locate the global square's centre and crossing line in the queried frame,
// rather than assuming how OCCT chose a planar face's parameter origin.
QJsonArray inSketch(const mitcad::Document& doc, const std::array<double, 3>& point) {
  const auto frame = sketch(doc).value("frame").toObject();
  const auto origin = frame.value("origin").toArray();
  const auto x = frame.value("x_axis").toArray();
  const auto y = frame.value("y_axis").toArray();
  require(origin.size() == 3 && x.size() == 3 && y.size() == 3, "invalid sketch frame");
  double u = 0;
  double v = 0;
  for (int i = 0; i < 3; ++i) {
    const double delta = point[static_cast<std::size_t>(i)] - origin.at(i).toDouble();
    u += delta * x.at(i).toDouble();
    v += delta * y.at(i).toDouble();
  }
  return {u, v};
}

void openCircle(mitcad::Document& doc) {
  onFace(doc, patchFace(doc));
  command(doc, {{"cmd", "sketch.add_circle"}, {"sketch", "F3"},
                {"center", inSketch(doc, {0, 0, 0})}, {"diameter", 4}});
}

QJsonObject openOutside(const mitcad::Document& doc, double faceArea = 100) {
  const auto state = sketch(doc);
  const auto regions = state.value("regions").toArray();
  require(regions.size() == 2, "open Patch and circle must give exactly two regions");
  require(state.value("entities").toArray().size() == 2 && state.value("projections").toArray().isEmpty(),
          "open Patch boundary was persisted as projected/sketch entities");
  QJsonObject outsideRegion;
  int bounded = 0;
  for (const auto& value : regions) {
    const auto region = value.toObject();
    if (region.value("face_bounded").toBool()) {
      outsideRegion = region;
      ++bounded;
    } else {
      checkNear(region.value("area").toDouble(), 4 * kPi, "open Patch inner circle area");
    }
  }
  require(bounded == 1, "open Patch outside region lost its face_bounded mark");
  require(outsideRegion.value("key").toString().contains("b{O{"), "open Patch key lost stable open boundary names");
  checkNear(outsideRegion.value("area").toDouble(), faceArea - 4 * kPi, "open Patch outside region area");
  return outsideRegion;
}

void checkStoredOpenSketch(const mitcad::Document& doc) {
  const auto saved = json(doc.to_json()).toObject();
  bool found = false;
  for (const auto& value : saved.value("features").toArray()) {
    const auto feature = value.toObject();
    if (feature.value("uid").toString() != "F3") continue;
    found = true;
    require(feature.value("entities").toArray().size() == 2, "file stored implicit boundary entities");
    require(feature.value("projections").toArray().isEmpty(), "file stored an implicit projection");
    require(!QJsonDocument(feature).toJson().contains("b{O{"), "file stored transient open edges in its sketch");
  }
  require(found, "saved file lost the face sketch");
}

void openPatchTest() {
  auto doc = mitcad::new_document();
  openCircle(*doc);
  const auto outsideRegion = openOutside(*doc);
  const auto key = outsideRegion.value("key").toString();
  const auto hash = profile(*doc, key).value("hash");
  require(profile(*doc, key).value("face_bounded").toBool(), "profiles omitted open Patch boundary mark");
  const auto shape = doc->profile_shape("F3", key.toStdString());
  require(bool(shape), "open Patch outside region has no real OCCT face");
  checkNear(mitcad::geometry::mass_properties(*shape).area, 100 - 4 * kPi, "OCCT open Patch profile area");
  command(*doc, extrusion("F3", key, "new_body", 3));
  require(query(*doc, {{"query", "bodies"}}).toArray().size() == 2,
          "New Body extrusion did not preserve its supporting open surface");
  auto solid = doc->body_shape("F4.b0");
  require(bool(solid), "open Patch extrusion did not create a solid");
  checkNear(mitcad::geometry::volume(*solid), (100 - 4 * kPi) * 3, "open Patch New Body volume");
  checkStoredOpenSketch(*doc);

  // Persist the consumer's dynamic region key, then reconstruct the whole
  // procedural history through the same JSON path a saved file uses.
  auto reopened = mitcad::load_document(std::string(doc->to_json()));
  command(*reopened, {{"cmd", "recompute"}});
  require(openOutside(*reopened).value("key").toString() == key, "save/reopen renamed open Patch profile");
  solid = reopened->body_shape("F4.b0");
  require(bool(solid), "save/reopen lost the open Patch profile consumer");
  checkNear(mitcad::geometry::volume(*solid), (100 - 4 * kPi) * 3, "reopened open Patch New Body volume");

  const auto dimensions = query(*reopened, {{"query", "sketch"}, {"uid", "F1"}})
                              .toObject().value("dimensions").toArray();
  require(dimensions.size() == 2, "source rectangle lost its two driving dimensions");
  const auto parameter = dimensions.first().toObject().value("value").toString();
  require(!parameter.isEmpty(), "source dimension has no parameter");
  command(*reopened, {{"cmd", "set_parameter"}, {"name", parameter}, {"value", 12}});
  require(openOutside(*reopened, 120).value("key").toString() == key, "source size change renamed open boundary key");
  require(profile(*reopened, key).value("hash") != hash, "source size change kept stale open boundary geometry");
  solid = reopened->body_shape("F4.b0");
  require(bool(solid), "source size change lost the New Body consumer");
  checkNear(mitcad::geometry::volume(*solid), (120 - 4 * kPi) * 3, "resized open Patch New Body volume");
  checkStoredOpenSketch(*reopened);
  auto resized = mitcad::load_document(std::string(reopened->to_json()));
  command(*resized, {{"cmd", "recompute"}});
  require(openOutside(*resized, 120).value("key").toString() == key, "resized save/reopen changed region key");
  solid = resized->body_shape("F4.b0");
  require(bool(solid), "resized save/reopen lost the profile consumer");
  checkNear(mitcad::geometry::volume(*solid), (120 - 4 * kPi) * 3, "resized reopened New Body volume");

  command(*resized, {{"cmd", "sketch.redefine_plane"}, {"sketch", "F3"}, {"plane", "xy"}});
  const auto own = sketch(*resized).value("regions").toArray();
  require(own.size() == 1 && !own.first().toObject().value("face_bounded").toBool(),
          "Redefine Sketch Plane kept an implicit open Patch profile");
  checkNear(own.first().toObject().value("area").toDouble(), 4 * kPi, "off-face own circle area");
  require(query(*resized, {{"query", "profile"}, {"sketch", "F3"}, {"region", key}}).isNull(),
          "lost open boundary profile rebound to the circle");
  const auto lost = query(*resized, {{"query", "feature"}, {"uid", "F4"}}).toObject();
  require(lost.value("status").toString() == "error" && lost.value("error").toString().contains("has no profile"),
          "open Patch consumer did not report a missing profile");
}

void openCrossingTest() {
  auto doc = mitcad::new_document();
  onFace(*doc, patchFace(*doc));
  command(*doc, {{"cmd", "sketch.add_line"}, {"sketch", "F3"},
                 {"start", inSketch(*doc, {-8, 0, 0})}, {"end", inSketch(*doc, {8, 0, 0})}});
  const auto regions = sketch(*doc).value("regions").toArray();
  require(regions.size() == 2, "crossing line did not split the open Patch into two profiles");
  QStringList keys;
  for (const auto& value : regions) {
    const auto region = value.toObject();
    require(region.value("face_bounded").toBool(), "open half-Patch lost its boundary mark");
    keys.append(region.value("key").toString());
    checkNear(region.value("area").toDouble(), 50, "open half-Patch area");
    const auto shape = doc->profile_shape("F3", region.value("key").toString().toStdString());
    require(bool(shape), "open half-Patch has no OCCT profile");
    checkNear(mitcad::geometry::mass_properties(*shape).area, 50, "OCCT open half-Patch area");
  }
  require(sketch(*doc).value("entities").toArray().size() == 3, "open boundary added entities beside the line");
  const auto dimensions = query(*doc, {{"query", "sketch"}, {"uid", "F1"}})
                              .toObject().value("dimensions").toArray();
  require(!dimensions.isEmpty(), "source rectangle has no driving dimension");
  // add_rectangle creates width before height. Widening preserves the
  // horizontal crossing line through both halves of the fixed-height face.
  command(*doc, {{"cmd", "set_parameter"},
                 {"name", dimensions.first().toObject().value("value").toString()}, {"value", 12}});
  const auto resized = sketch(*doc).value("regions").toArray();
  require(resized.size() == 2, "source change lost an open half-Patch");
  for (const auto& value : resized) {
    const auto region = value.toObject();
    require(keys.contains(region.value("key").toString()), "source change renamed an open half-Patch");
    checkNear(region.value("area").toDouble(), 60, "resized open half-Patch area");
  }
  command(*doc, {{"cmd", "sketch.redefine_plane"}, {"sketch", "F3"}, {"offset", 2}});
  require(sketch(*doc).value("regions").toArray().isEmpty(), "offset open line kept implicit Patch regions");
}

void unsupportedOpenFaceTest() {
  // A swept open line has a planar wall, but its free edges have no
  // source-profile segment provenance. It must not invent Patch names.
  auto doc = mitcad::new_document();
  command(*doc, {{"cmd", "sketch.create"}});
  const auto made = command(*doc, {{"cmd", "sketch.add_line"}, {"sketch", "F1"},
      {"start", QJsonArray{-5, 0}}, {"end", QJsonArray{5, 0}}}).toObject().value("made").toArray();
  require(made.size() == 1, "unsupported-face fixture did not create one line");
  command(*doc, {{"cmd", "add_feature"}, {"def", QJsonObject{{"type", "extrude"}, {"surface", true},
      {"operation", "new_body"}, {"curves", QJsonObject{{"sketch", "F1"}, {"curves", made}}},
      {"extent", QJsonObject{{"type", "distance"}, {"distance", 10}}}}}});
  const auto faces = query(*doc, {{"query", "faces"}, {"body", "F2.b0"}, {"geometry", true}}).toArray();
  require(faces.size() == 1 && faces.first().toObject().value("surface").toString() == "plane",
          "unsupported-face fixture is not one planar surface");
  const auto names = faces.first().toObject().value("names").toArray();
  require(!names.isEmpty(), "unsupported-face fixture has no named face");
  onFace(*doc, names.first().toString());
  command(*doc, {{"cmd", "sketch.add_circle"}, {"sketch", "F3"},
                {"center", inSketch(*doc, {0, 0, 5})}, {"diameter", 4}});
  const auto state = sketch(*doc);
  const auto regions = state.value("regions").toArray();
  require(regions.size() == 1 && !regions.first().toObject().value("face_bounded").toBool(),
          "unsupported open face invented implicit regions");
  checkNear(regions.first().toObject().value("area").toDouble(), 4 * kPi, "unsupported face own circle area");
  bool warned = false;
  for (const auto& warning : state.value("warnings").toArray()) {
    const auto text = warning.toString();
    warned |= text.contains("face-bounded profiles unavailable") && text.contains("unnamed edge");
  }
  require(warned, "unsupported open boundary did not give its explicit warning");
}

void copiedBoundaryConsumerTest(bool openSupport) {
  auto doc = mitcad::new_document();
  const auto created = command(*doc, {{"cmd", "create_component"}, {"name", "Source"}, {"activate", true}})
                           .toObject();
  QString key;
  double expected = 0;
  if (openSupport) {
    openCircle(*doc);
    key = openOutside(*doc).value("key").toString();
    command(*doc, extrusion("F3", key, "new_body", 3));
    expected = (100 - 4 * kPi) * 3;
  } else {
    onFace(*doc, circleFace(*doc));
    key = polygon(*doc);
    command(*doc, extrusion("F3", key, "join", 2));
    expected = joinedVolume(30);
  }
  command(*doc, {{"cmd", "activate_component"}, {"component", "C0"}});
  const auto pasted = command(*doc, {{"cmd", "paste_new"}, {"occurrence", created.value("occurrence")}})
                          .toObject();
  const auto component = pasted.value("component").toString();
  require(!component.isEmpty() && component != created.value("component").toString(),
          "Paste New did not create an independent component");
  QJsonArray copied;
  for (const auto& value : query(*doc, {{"query", "timeline"}}).toObject().value("features").toArray()) {
    const auto item = value.toObject();
    if (item.value("component").toString() == component) copied.append(item);
  }
  require(copied.size() == 4, "Paste New did not copy the four-feature profile history");
  const auto source = copied.at(1).toObject().value("uid").toString();
  const auto sketchUid = copied.at(2).toObject().value("uid").toString();
  const auto consumer = copied.at(3).toObject().value("uid").toString();
  const auto feature = query(*doc, {{"query", "feature"}, {"uid", consumer}}).toObject();
  require(feature.value("status").toString() == "ok", "copied boundary consumer did not evaluate");
  const auto selection = feature.value("def").toObject().value("profiles").toArray().first().toObject();
  require(selection.value("sketch").toString() == sketchUid, "copied consumer kept the original sketch");
  const auto copiedKey = selection.value("region").toString();
  require(copiedKey != key && copiedKey.contains(source + ":") && !copiedKey.contains("F2:"),
          "copied boundary region kept its original support owner");
  const auto body = (openSupport ? consumer : source) + ".b0";
  auto shape = doc->body_shape(body.toStdString());
  require(bool(shape), "Paste New lost its copied boundary solid");
  checkNear(mitcad::geometry::volume(*shape), expected, "copied boundary consumer volume");
  command(*doc, {{"cmd", "set_parameter"}, {"name", "d1"}, {"value", openSupport ? 12 : 80}});
  shape = doc->body_shape(body.toStdString());
  require(bool(shape), "changing the original support lost the copied solid");
  checkNear(mitcad::geometry::volume(*shape), expected, "copied support parameters are independent");
  auto reopened = mitcad::load_document(std::string(doc->to_json()));
  command(*reopened, {{"cmd", "recompute"}});
  shape = reopened->body_shape(body.toStdString());
  require(bool(shape), "reopening lost the copied boundary solid");
  checkNear(mitcad::geometry::volume(*shape), expected, "reopened copied boundary consumer volume");
  const auto reopenedConsumer = query(*reopened, {{"query", "feature"}, {"uid", consumer}}).toObject();
  require(reopenedConsumer.value("def").toObject().value("profiles").toArray().first().toObject()
              .value("region").toString() == copiedKey, "reopening changed the copied boundary key");
}

void write(const QString& file, const QByteArray& bytes) {
  QFile output(file);
  require(output.open(QIODevice::WriteOnly), "cannot create fixture file");
  require(output.write(bytes) == bytes.size(), "cannot write fixture file");
}

void fixtures(const QString& folder, const QString& scripts) {
  require(QDir().mkpath(folder), "cannot create fixture directory");
  auto doc = mitcad::new_document();
  const auto face = circleFace(*doc);
  write(QDir(folder).filePath("whole-face.mitcad"), QByteArray::fromStdString(std::string(doc->to_json())));
  onFace(*doc, face);
  const auto key = polygon(*doc);
  write(QDir(folder).filePath("face-profiles.mitcad"), QByteArray::fromStdString(std::string(doc->to_json())));
  for (const auto& name : {QStringLiteral("face-bounded-profiles"), QStringLiteral("face-bounded-whole-face")}) {
    QFile source(QDir(scripts).filePath(name + ".mitcad-ui"));
    require(source.open(QIODevice::ReadOnly), "cannot open shared UI workflow");
    QString text = QString::fromUtf8(source.readAll());
    text.replace("@OUTSIDE_PICK@", QRegularExpression::escape("profile F3/" + key));
    text.replace("@WHOLE_FACE_PICK@", QRegularExpression::escape("face F2.b0/" + face));
    text.replace("@JOINED_VOLUME@", QString::number(joinedVolume(30), 'g', 17));
    text.replace("@BASE_VOLUME@", QString::number(kPi * 9000, 'g', 17));
    write(QDir(folder).filePath(name + ".mitcad-ui"), text.toUtf8());
  }
  auto open = mitcad::new_document();
  openCircle(*open);
  const auto openRegion = openOutside(*open);
  write(QDir(folder).filePath("open-face-profiles.mitcad"), QByteArray::fromStdString(std::string(open->to_json())));
  QFile source(QDir(scripts).filePath("face-bounded-open-face.mitcad-ui"));
  require(source.open(QIODevice::ReadOnly), "cannot open open-face UI workflow");
  QString text = QString::fromUtf8(source.readAll());
  text.replace("@OPEN_PICK@", QRegularExpression::escape("profile F3/" + openRegion.value("key").toString()));
  text.replace("@OPEN_VOLUME@", QString::number((100 - 4 * kPi) * 3, 'g', 17));
  write(QDir(folder).filePath("face-bounded-open-face.mitcad-ui"), text.toUtf8());
}

} // namespace

int main(int argc, char** argv) {
  QCoreApplication application(argc, argv);
  try {
    if (argc == 4 && std::string(argv[1]) == "--write-fixture") {
      fixtures(QString::fromLocal8Bit(argv[2]), QString::fromLocal8Bit(argv[3]));
    } else {
      require(argc == 1, "usage: test_face_profiles [--write-fixture <directory> <UI scripts directory>]");
      ringTest();
      crossingTest();
      openPatchTest();
      openCrossingTest();
      unsupportedOpenFaceTest();
      copiedBoundaryConsumerTest(true);
      copiedBoundaryConsumerTest(false);
    }
    std::puts("PASS: synthetic face-bounded profiles");
    return 0;
  } catch (const std::exception& error) {
    std::fprintf(stderr, "FAIL: face-bounded profiles: %s\n", error.what());
    return 1;
  }
}
