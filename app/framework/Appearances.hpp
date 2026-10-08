// SPDX-License-Identifier: MIT
#pragma once

// Appearances (mitcad#46): the model keeps Mitcad's library and the
// document's own appearances as physically based parameter sets
// (core/model/src/appearance.rs, the `appearances` query); a body names one
// by id (`set_body_appearance`), and so can single faces
// (`set_face_appearance`, mitcad#53). The shaded view colours a body or a
// face with the appearance's display colour, the rendered view uses every
// parameter and the texture. Physical materials are the model's own list
// (`steel`, ...).

#include <functional>

#include <QByteArray>
#include <QColor>
#include <QHash>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

namespace mitcad {

// An appearance's texture as the `appearances` query lists it (mitcad#53):
// an image for the base colour, projected onto the body.
struct AppearanceTexture {
  QJsonObject json; // as listed: sent back as it is, it keeps an embedded image
  QString path;     // the image file, relative to the design's folder or absolute
  double width = 100.0; // one repeat of the image, mm
  double height = 100.0;
  double rotation = 0.0; // radians
  bool planar = false;   // else box
  bool embedded = false; // the image is in the project file
  QString imageSha256;   // of an embedded image
  // The image file the renderer reads (resolveTexture): absolute; empty
  // when there is none, with the reason in `missing`.
  QString file = QString();
  QString missing = QString();

  bool operator==(const AppearanceTexture& o) const {
    return path == o.path && width == o.width && height == o.height && rotation == o.rotation &&
           planar == o.planar && embedded == o.embedded && imageSha256 == o.imageSha256 && file == o.file;
  }
  bool operator!=(const AppearanceTexture& o) const { return !(*this == o); }
};

// An appearance as the `appearances` query lists it. Colours are sRGB.
struct Appearance {
  QString id;
  QString name;
  bool library = true;
  QColor baseColor = QColor::fromRgbF(0.8f, 0.8f, 0.8f);
  double metalness = 0.0;
  double roughness = 0.5;
  double specular = 1.0;
  double transmission = 0.0;
  double ior = 1.5;
  double coat = 0.0;
  double coatRoughness = 0.03;
  double emission = 0.0;
  QColor emissionColor = QColor(Qt::white);
  double opacity = 1.0;
  // The texture: the rendered view draws it, the shaded view shows the
  // base colour.
  bool hasTexture = false;
  AppearanceTexture texture = AppearanceTexture();
  // The shaded view's colour (the base colour).
  QColor displayColor = baseColor;
  // Bodies that use it, and faces (body uid, face name as assigned).
  QStringList bodies;
  QVector<QPair<QString, QString>> faces = {};

  // The same look: every parameter equal (not the name or the users).
  bool sameLook(const Appearance& other) const;
};

// The model's appearances (the `appearances` query: the library first).
QVector<Appearance> appearancesOf(const QJsonArray& query);
Appearance appearanceOf(const QJsonObject& value);
// The parameters as `create_appearance` / `edit_appearance` fields.
QJsonObject appearanceFields(const Appearance& appearance);
// By id; null for none.
const Appearance* findAppearance(const QVector<Appearance>& appearances, const QString& id);
// The appearances by id, for looking up many bodies.
QHash<QString, Appearance> appearancesById(const QVector<Appearance>& appearances);

// A preview swatch: a sphere lit by a soft sky and a light from the upper
// left, over a checkerboard that shows transmission and opacity; `size`
// device-independent pixels at the device pixel ratio.
QImage appearanceSwatch(const Appearance& appearance, int size, qreal devicePixelRatio = 1.0);

// Finds the image file of a texture for the renderer (mitcad#53): its path
// made absolute against the design's folder (empty for an unsaved design:
// a relative path is then missing), or an embedded image, written once per
// session into a temporary folder by its digest; `embeddedImage` gives its
// bytes when it is not there yet (the `appearance_image` query). Sets
// `file`, or `missing` with the reason.
void resolveTexture(AppearanceTexture& texture, const QString& documentFolder,
                    const std::function<QByteArray()>& embeddedImage);

// sRGB in [0, 1] as JSON and back.
QJsonArray colorJson(const QColor& color);
QColor colorOf(const QJsonValue& value);

// The model's physical materials (id, name), steel first as the default.
const QVector<QPair<QString, QString>>& physicalMaterials();

} // namespace mitcad
