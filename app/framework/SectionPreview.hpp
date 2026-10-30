// SPDX-License-Identifier: MIT
#pragma once

// Section Analysis and Hide Above Sketch over a feature preview
// (mitcad#943). The view shows the bodies cut at a plane: the window clips
// the bodies at the marker (MainWindow::displayBodies, the model's
// analysis_clips), and a command's preview, which draws the bodies it
// changes, moves or makes with their new shapes, has those clipped by the
// same clip (the clips request's `preview` and `transform`, commands.md),
// off the UI thread as the section's rounds (framework/SectionWorker.hpp).
// Until a preview's clips are ready, each body it clips shows the last clip
// of a preview drawn for it, or the clip the view had before the command:
// never the uncut body. The caps are the faces the clip made in the plane,
// for the committed bodies and a preview's alike.

#include <memory>
#include <string>
#include <vector>

#include <QHash>
#include <QJsonObject>
#include <QJsonValue>
#include <QSet>
#include <QString>

#include <TopLoc_Location.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

namespace mitcad {

namespace geometry {
class Shape;
} // namespace geometry

// A body as the view draws it at a cut: what it draws, the shape the clip
// cut (null for a body not cut) and whether the plane cut it (drawn behind
// the caps, BodyDisplay::sectioned).
struct SectionedShape {
  std::shared_ptr<geometry::Shape> shape;
  std::shared_ptr<geometry::Shape> uncut;
  bool sectioned = false;
};

// A body of a preview to clip: by its key (previewBodyKey), the model's
// preview of the body (`preview`; else the body at the marker, which the
// preview only places anew) and where the view places it.
struct PreviewClipTarget {
  QString key;
  std::string uid;
  bool preview = true;
  TopLoc_Location placement;
};

// The clips of a preview's bodies at a plane, by previewBodyKey; shared by
// the preview, its cached copies and the round that fills it.
struct PreviewSection {
  // The plane (design coordinates, as the window's cut plane).
  QJsonValue plane;
  // The bodies clipped.
  QSet<QString> keys;
  // Set once the round is done (on the UI thread).
  bool ready = false;
  // Once ready: what is left of each body, the clipped shape itself when
  // the plane leaves it whole (an empty shape: nothing left); null when
  // its clip failed, the body then drawn whole (logged as "Section: <uid>
  // shown whole: <error>", mitcad#392).
  QHash<QString, std::shared_ptr<geometry::Shape>> left;
  // The shape each clip cut: the model's preview of the body, or the body
  // at the marker.
  QHash<QString, std::shared_ptr<geometry::Shape>> uncut;
  // What each body shows until the clips are ready; one without an entry
  // (a body the preview makes, first drawn) shows nothing meanwhile.
  QHash<QString, SectionedShape> meanwhile;
};

// A body's key in a preview's clips: its uid and occurrence.
QString previewBodyKey(const std::string& uid, const std::string& occurrence);

// A body of a clips request (analysis_clips, prepare_clips): the model's
// preview of it (`preview`) or the body at the marker, placed in the
// design by `placement` (a rigid placement; the identity in the design's
// own coordinates).
QJsonObject previewClipTarget(const QString& uid, bool preview, const TopLoc_Location& placement);

// What a body draws of its clip `left` (PreviewSection::left) of `uncut`,
// `drawn` being what it draws whole (a stand-in of the interactive tier,
// or the body): `drawn` when the clip failed (null) or left it whole (the
// shape itself), else the clip, cut.
SectionedShape sectionedShape(const std::shared_ptr<geometry::Shape>& drawn,
                              const std::shared_ptr<geometry::Shape>& uncut,
                              const std::shared_ptr<geometry::Shape>& left);

// The plane of a cut ({"origin": [x, y, z], "normal": [x, y, z]} in design
// coordinates); false for none or a zero normal.
bool cutPlaneOf(const QJsonValue& plane, gp_Pnt& origin, gp_Dir& normal);

// The caps of a body cut at the plane through `origin` with the normal
// (design coordinates): the faces of its clip `cut` in the plane, in the
// body's coordinates, without those its `uncut` shape has there too (a
// sketch on the body's face; a null `uncut`: none). `placement` puts the
// body in the design. Faces without a surface (a mesh body's triangles)
// are none.
std::vector<TopoDS_Face> sectionCapFaces(const TopoDS_Shape& cut, const TopoDS_Shape& uncut,
                                         const TopLoc_Location& placement, const gp_Pnt& origin,
                                         const gp_Dir& normal);

// The faces' area together (mm²).
double facesArea(const std::vector<TopoDS_Face>& faces);

} // namespace mitcad
