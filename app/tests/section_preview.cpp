// SPDX-License-Identifier: MIT
// Section Analysis over a feature preview (mitcad#943), without a window:
// a 30 x 20 x 10 mm block (F2.b0) cut at x = 10, its normal along +X, and
// a preview that joins a 10 x 5 x 5 mm boss across the plane on its top
// (x 5 to 15, y 5 to 10). The preview's body is clipped by the clip the
// window's bodies take (Document::analysis_clips, and prepare_clips run as
// the section worker runs it), asked for as the window asks
// (previewClipTarget); its caps follow the preview: 200 mm² for the block
// at the marker, 225 mm² with the boss. Also: a body the preview makes, a
// body placed elsewhere in the design, one the plane leaves whole, one it
// removes, and one whose clip fails.
#include <cmath>
#include <cstdio>
#include <memory>
#include <string>

#include <QByteArray>
#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>

#include <BRepBndLib.hxx>
#include <Bnd_Box.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "framework/Json.hpp"
#include "framework/SectionPreview.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/lib.h"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "section_preview.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::Document;
using mitcad::parseObject;
using mitcad::rustStr;
using ShapePtr = std::shared_ptr<mitcad::geometry::Shape>;

const char* const kRectangle = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

QJsonObject run(Document& document, const QJsonObject& command) {
  return parseObject(document.command(rustStr(mitcad::compactJson(command))));
}

QJsonObject rectangle(const QString& sketch, double x, double y, double width, double height) {
  return QJsonObject{{"cmd", "sketch.add_rectangle"},
                     {"sketch", sketch},
                     {"corner", QJsonArray{x, y}},
                     {"width", width},
                     {"height", height}};
}

// An extrusion of a sketch's rectangle, `offset` above its sketch.
QJsonObject extrude(const QString& sketch, double offset, double distance, const char* operation) {
  QJsonArray profiles;
  profiles.append(QJsonObject{{"sketch", sketch}, {"region", kRectangle}});
  return QJsonObject{{"cmd", "add_feature"},
                     {"def", QJsonObject{{"type", "extrude"},
                                         {"profiles", profiles},
                                         {"start", QJsonObject{{"type", "offset"}, {"offset", offset}}},
                                         {"extent", QJsonObject{{"type", "distance"}, {"distance", distance}}},
                                         {"operation", operation}}}};
}

// The plane x = `x`, its normal along +X: what is beyond it is cut away.
QJsonValue planeAt(double x) {
  return QJsonObject{{"origin", QJsonArray{x, 0.0, 0.0}}, {"normal", QJsonArray{1.0, 0.0, 0.0}}};
}

struct Clip {
  ShapePtr shape;
  std::string error;
};

// One body's clip as the window asks for it: `prepared` as the section
// worker works it out (prepare_clips, then run), else at once.
Clip clipOf(const Document& document, const QJsonValue& plane, const QString& uid, bool preview,
            const TopLoc_Location& placement, bool prepared) {
  QJsonArray bodies;
  bodies.append(mitcad::previewClipTarget(uid, preview, placement));
  const QByteArray request = mitcad::compactJson(QJsonObject{{"plane", plane}, {"bodies", bodies}});
  Clip clip;
  try {
    const rust::Box<mitcad::ClippedBodies> results =
        prepared ? document.prepare_clips(rustStr(request))->run() : document.analysis_clips(rustStr(request));
    if (results->count() == 1) {
      clip.shape = results->shape(0);
      const rust::String error = results->error(0);
      clip.error = std::string(error.data(), error.size());
    } else {
      clip.error = "no result";
    }
  } catch (const std::exception& e) {
    clip.error = e.what();
  }
  return clip;
}

// The caps' area of a body cut at x = `x` and drawn at `placement`.
double capArea(const ShapePtr& cut, const ShapePtr& uncut, const TopLoc_Location& placement, double x) {
  gp_Pnt origin;
  gp_Dir normal;
  if (!mitcad::cutPlaneOf(planeAt(x), origin, normal) || !cut) {
    return -1.0;
  }
  return mitcad::facesArea(
      mitcad::sectionCapFaces(cut->occt(), uncut ? uncut->occt() : TopoDS_Shape(), placement, origin, normal));
}

// How far along X the shape reaches, in its own coordinates.
double maxX(const ShapePtr& shape) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(shape->occt(), box, false, false);
  return box.IsVoid() ? -1e9 : box.CornerMax().X();
}

bool about(double a, double b) { return std::abs(a - b) < 1e-6; }

bool hasFaces(const ShapePtr& shape) { return TopExp_Explorer(shape->occt(), TopAbs_FACE).More(); }

void testHelpers() {
  CHECK(mitcad::previewBodyKey("F2.b0", "O1") == QStringLiteral("F2.b0@O1"));
  gp_Pnt origin;
  gp_Dir normal;
  CHECK(mitcad::cutPlaneOf(planeAt(10.0), origin, normal));
  CHECK(about(origin.X(), 10.0) && about(normal.X(), 1.0));
  CHECK(!mitcad::cutPlaneOf(QJsonValue(), origin, normal));
  CHECK(!mitcad::cutPlaneOf(QJsonObject{{"origin", QJsonArray{0.0, 0.0, 0.0}}, {"normal", QJsonArray{0.0, 0.0, 0.0}}},
                            origin, normal));
  // The request's body: the preview's, placed by the view's rigid placement.
  gp_Trsf moved;
  moved.SetTranslation(gp_Vec(4.0, 0.0, 0.0));
  const QJsonObject target = mitcad::previewClipTarget(QStringLiteral("F2.b0"), true, TopLoc_Location(moved));
  CHECK(target.value("body").toString() == QStringLiteral("F2.b0"));
  CHECK(target.value("preview").toBool());
  const QJsonObject transform = target.value("transform").toObject();
  CHECK(transform.value("translation").toArray().at(0).toDouble() == 4.0);
  CHECK(transform.value("linear").toArray().at(1).toArray().at(1).toDouble() == 1.0);
  CHECK(!mitcad::previewClipTarget(QStringLiteral("F2.b0"), false, TopLoc_Location()).contains("preview"));
}

void testPreviewClips() {
  rust::Box<Document> document = mitcad::new_document();
  const QString block = run(*document, QJsonObject{{"cmd", "sketch.create"}}).value("uid").toString();
  run(*document, rectangle(block, 0, 0, 30, 20));
  run(*document, extrude(block, 0, 10, "new_body"));
  const QString boss = run(*document, QJsonObject{{"cmd", "sketch.create"}}).value("uid").toString();
  run(*document, rectangle(boss, 5, 5, 10, 5));
  const std::string uid = "F2.b0";
  const QString body = QString::fromStdString(uid);
  const TopLoc_Location here;

  // The body at the marker, as the window clips it (displayBodies).
  const ShapePtr whole = document->body_shape(uid);
  CHECK(whole != nullptr);
  const Clip committed = clipOf(*document, planeAt(10.0), body, false, here, false);
  CHECK(committed.shape && committed.error.empty());
  CHECK(committed.shape && !committed.shape->occt().IsSame(whole->occt()));
  CHECK(about(capArea(committed.shape, whole, here, 10.0), 200.0));

  // The preview of the boss joined: its body clipped by the same clip.
  const QJsonObject report =
      parseObject(document->preview(rustStr(mitcad::compactJson(extrude(boss, 10, 5, "join")))));
  CHECK(report.value("status").toString() == QStringLiteral("ok"));
  const ShapePtr previewed = document->preview_body_shape(uid);
  CHECK(previewed != nullptr);
  if (!previewed) {
    return;
  }
  CHECK(!previewed->occt().IsSame(whole->occt()));
  const bool ways[] = {false, true}; // at once, then as the section worker
  for (const bool prepared : ways) {
    const Clip clip = clipOf(*document, planeAt(10.0), body, true, here, prepared);
    CHECK(clip.shape && clip.error.empty());
    if (!clip.shape) {
      continue;
    }
    CHECK(!clip.shape->occt().IsSame(previewed->occt()));
    CHECK(maxX(clip.shape) < 10.0 + 1e-6);
    // The caps follow the preview: the block's and the boss's cut.
    CHECK(about(capArea(clip.shape, previewed, here, 10.0), 225.0));
    // Drawn cut, with the preview's body as the shape it cut.
    const mitcad::SectionedShape shown = mitcad::sectionedShape(previewed, previewed, clip.shape);
    CHECK(shown.sectioned && shown.shape == clip.shape && shown.uncut == previewed);
  }

  // Placed 4 mm along X in the design: cut at x = 6 of its own.
  gp_Trsf moved;
  moved.SetTranslation(gp_Vec(4.0, 0.0, 0.0));
  const TopLoc_Location placement(moved);
  const Clip placed = clipOf(*document, planeAt(10.0), body, true, placement, true);
  CHECK(placed.shape && placed.error.empty());
  if (placed.shape) {
    CHECK(about(maxX(placed.shape), 6.0));
    CHECK(about(capArea(placed.shape, previewed, placement, 10.0), 225.0));
  }

  // A plane past the body leaves it whole: drawn as it is, not cut.
  const Clip kept = clipOf(*document, planeAt(50.0), body, true, here, true);
  CHECK(kept.shape && kept.shape->occt().IsSame(previewed->occt()));
  const mitcad::SectionedShape keptShown = mitcad::sectionedShape(previewed, previewed, kept.shape);
  CHECK(!keptShown.sectioned && keptShown.shape == previewed && !keptShown.uncut);
  // A plane before it removes all of it: drawn as nothing.
  const Clip removed = clipOf(*document, planeAt(-5.0), body, true, here, false);
  CHECK(removed.shape && !hasFaces(removed.shape));
  const mitcad::SectionedShape removedShown = mitcad::sectionedShape(previewed, previewed, removed.shape);
  CHECK(removedShown.sectioned && removedShown.shape == removed.shape);
  CHECK(about(capArea(removed.shape, previewed, here, -5.0), 0.0));
  // A clip that fails (no such body in the preview): drawn whole.
  const Clip failed = clipOf(*document, planeAt(10.0), QStringLiteral("F99.b0"), true, here, true);
  CHECK(!failed.shape && !failed.error.empty());
  const mitcad::SectionedShape failedShown = mitcad::sectionedShape(previewed, nullptr, failed.shape);
  CHECK(!failedShown.sectioned && failedShown.shape == previewed);

  // A body the preview makes (New Body), which is no body at the marker.
  const QString side = run(*document, QJsonObject{{"cmd", "sketch.create"}}).value("uid").toString();
  run(*document, rectangle(side, 5, 30, 10, 10));
  const QJsonObject made =
      parseObject(document->preview(rustStr(mitcad::compactJson(extrude(side, 0, 10, "new_body")))));
  CHECK(made.value("status").toString() == QStringLiteral("ok"));
  QString madeUid;
  for (const QJsonValue& value : made.value("bodies").toArray()) {
    const QString found = value.toObject().value("uid").toString();
    if (found != body) {
      madeUid = found;
    }
  }
  CHECK(!madeUid.isEmpty());
  const ShapePtr madeShape = document->preview_body_shape(madeUid.toStdString());
  CHECK(madeShape != nullptr);
  const Clip madeClip = clipOf(*document, planeAt(10.0), madeUid, true, here, true);
  CHECK(madeClip.shape && madeClip.error.empty());
  if (madeClip.shape && madeShape) {
    CHECK(maxX(madeClip.shape) < 10.0 + 1e-6);
    CHECK(about(capArea(madeClip.shape, madeShape, here, 10.0), 100.0));
  }
  // At the marker it is not there.
  CHECK(!clipOf(*document, planeAt(10.0), madeUid, false, here, false).error.empty());
}

} // namespace

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  testHelpers();
  testPreviewClips();
  if (failures > 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("section preview: all checks passed\n");
  return 0;
}
