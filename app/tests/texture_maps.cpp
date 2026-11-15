// SPDX-License-Identifier: MIT
// The maps of a texture set in the application (mitcad#314): the
// `appearances` query's maps read into AppearanceTexture, and the files
// resolveTextureMaps finds for the renderer: an embedded map asked for by
// its name and written once by its digest, a file relative to the
// design's folder, a missing file with its reason, while the other images
// stay found.
#include "framework/Appearances.hpp"

#include <cstdio>

#include <QByteArray>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "texture_maps.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

// The first bytes of a PNG file: enough for resolveTextureMaps, which
// only writes the bytes and picks the extension.
QByteArray pngBytes(char tag) { return QByteArray("\x89PNG\r\n\x1a\n", 8) + QByteArray(8, tag); }

void mapsTests() {
  QTemporaryDir folder;
  CHECK(folder.isValid());
  QFile own(folder.filePath(QStringLiteral("own_rough.png")));
  CHECK(own.open(QIODevice::WriteOnly) && own.write(pngBytes('r')) == 16);
  own.close();

  // As the `appearances` query lists a set: an embedded base colour and
  // normal map, a roughness file of the user's next to the design, a
  // metalness file that is not there, no occlusion map.
  const QJsonObject listed{
      {QStringLiteral("id"), QStringLiteral("oak")},
      {QStringLiteral("name"), QStringLiteral("Oak")},
      {QStringLiteral("library"), false},
      {QStringLiteral("base_color"), QJsonArray{0.5, 0.4, 0.3}},
      {QStringLiteral("texture"),
       QJsonObject{
           {QStringLiteral("path"), QStringLiteral("oak_diff.png")},
           {QStringLiteral("embedded"), true},
           {QStringLiteral("image_sha256"), QStringLiteral("aa01")},
           {QStringLiteral("size"), QJsonArray{300.0, 200.0}},
           {QStringLiteral("normal"), QJsonObject{{QStringLiteral("path"), QStringLiteral("oak_nor_gl.png")},
                                                  {QStringLiteral("embedded"), true},
                                                  {QStringLiteral("image_sha256"), QStringLiteral("bb02")}}},
           {QStringLiteral("roughness"), QJsonObject{{QStringLiteral("path"), QStringLiteral("own_rough.png")}}},
           {QStringLiteral("metalness"), QJsonObject{{QStringLiteral("path"), QStringLiteral("gone/metal.png")}}},
       }}};
  Appearance a = appearanceOf(listed);
  CHECK(a.hasTexture);
  CHECK(QLatin1String(textureMapNames()[0]) == QLatin1String("normal") &&
        QLatin1String(textureMapNames()[3]) == QLatin1String("occlusion"));
  const AppearanceMap& normal = a.texture.maps[0];
  const AppearanceMap& roughness = a.texture.maps[1];
  const AppearanceMap& metalness = a.texture.maps[2];
  const AppearanceMap& occlusion = a.texture.maps[3];
  CHECK(normal.present && normal.embedded && normal.imageSha256 == QLatin1String("bb02"));
  CHECK(roughness.present && !roughness.embedded && roughness.path == QLatin1String("own_rough.png"));
  CHECK(metalness.present && metalness.path == QLatin1String("gone/metal.png"));
  CHECK(!occlusion.present);

  // The embedded images come by the map's name, once per digest.
  QStringList asked;
  const auto embedded = [&asked](const QString& map) {
    asked << map;
    return pngBytes(map == QLatin1String("normal") ? 'n' : 'c');
  };
  resolveTextureMaps(a.texture, folder.path(), embedded);
  CHECK(asked == (QStringList{QStringLiteral("base_color"), QStringLiteral("normal")}));
  CHECK(a.texture.file.endsWith(QLatin1String("aa01.png")) && a.texture.missing.isEmpty());
  CHECK(a.texture.maps[0].file.endsWith(QLatin1String("bb02.png")) && a.texture.maps[0].missing.isEmpty());
  QFile written(a.texture.maps[0].file);
  CHECK(written.open(QIODevice::ReadOnly) && written.readAll() == pngBytes('n'));
  CHECK(QFileInfo(a.texture.maps[1].file) == QFileInfo(own.fileName()));
  // The missing map is left out with its reason; the others are found.
  CHECK(a.texture.maps[2].file.isEmpty());
  CHECK(a.texture.maps[2].missing.contains(QLatin1String("is missing")) &&
        a.texture.maps[2].missing.contains(QLatin1String("metal.png")));
  CHECK(a.texture.maps[3].file.isEmpty() && a.texture.maps[3].missing.isEmpty());
  // Again: the images written before are used, nothing is asked.
  asked.clear();
  resolveTextureMaps(a.texture, folder.path(), embedded);
  CHECK(asked.isEmpty());
  CHECK(!a.texture.maps[0].file.isEmpty());

  // An unsaved design: a relative map path is missing, with the reason.
  Appearance unsaved = appearanceOf(listed);
  resolveTextureMaps(unsaved.texture, QString(), embedded);
  CHECK(unsaved.texture.maps[1].file.isEmpty() &&
        unsaved.texture.maps[1].missing.contains(QLatin1String("not saved yet")));
  CHECK(!unsaved.texture.file.isEmpty());

  // A map makes another look.
  Appearance other = appearanceOf(listed);
  CHECK(other.sameLook(appearanceOf(listed)));
  other.texture.maps[3].present = true;
  other.texture.maps[3].path = QStringLiteral("ao.png");
  CHECK(!other.sameLook(appearanceOf(listed)));
}

// An imported glTF material's appearance (mitcad#1862): laid by the body's
// texture coordinates (projection uv), one image's green channel the
// roughness, its blue the metalness, its red the occlusion, the three maps
// one image file for the renderer.
void packedMapsTests() {
  const QJsonObject packed{{QStringLiteral("path"), QStringLiteral("crate_mr.png")},
                           {QStringLiteral("embedded"), true},
                           {QStringLiteral("image_sha256"), QStringLiteral("cc03")}};
  const auto channel = [&packed](const char* name) {
    QJsonObject map = packed;
    map.insert(QStringLiteral("channel"), QLatin1String(name));
    return map;
  };
  // The roughness scaled by glTF's factor (mitcad#1874).
  QJsonObject roughness = channel("green");
  roughness.insert(QStringLiteral("factor"), 0.5);
  const QJsonObject listed{
      {QStringLiteral("id"), QStringLiteral("custom1")},
      {QStringLiteral("name"), QStringLiteral("Crate")},
      {QStringLiteral("library"), false},
      {QStringLiteral("base_color"), QJsonArray{1.0, 1.0, 1.0}},
      {QStringLiteral("texture"),
       QJsonObject{{QStringLiteral("path"), QStringLiteral("crate.png")},
                   {QStringLiteral("embedded"), true},
                   {QStringLiteral("image_sha256"), QStringLiteral("dd04")},
                   {QStringLiteral("projection"), QStringLiteral("uv")},
                   {QStringLiteral("roughness"), roughness},
                   {QStringLiteral("metalness"), channel("blue")},
                   {QStringLiteral("occlusion"), channel("red")}}}};
  Appearance a = appearanceOf(listed);
  CHECK(a.hasTexture && a.texture.uv && !a.texture.planar);
  CHECK(a.texture.maps[0].channel == -1);
  CHECK(a.texture.maps[1].channel == 1 && a.texture.maps[2].channel == 2 && a.texture.maps[3].channel == 0);
  // The factor read, 1 where a map has none (mitcad#1874).
  CHECK(a.texture.maps[1].factor == 0.5 && a.texture.maps[2].factor == 1.0);
  // Another channel makes another look, as another factor does.
  Appearance other = appearanceOf(listed);
  other.texture.maps[1].channel = 0;
  CHECK(!other.sameLook(a));
  Appearance unscaled = appearanceOf(listed);
  unscaled.texture.maps[1].factor = 1.0;
  CHECK(!unscaled.sameLook(a));
  // The one embedded image of the three maps is written once, by its
  // digest, and each map reads that file.
  QTemporaryDir folder;
  CHECK(folder.isValid());
  QStringList asked;
  resolveTextureMaps(a.texture, folder.path(), [&asked](const QString& map) {
    asked << map;
    return pngBytes('p');
  });
  CHECK(asked == (QStringList{QStringLiteral("base_color"), QStringLiteral("roughness")}));
  CHECK(a.texture.maps[1].file.endsWith(QLatin1String("cc03.png")));
  CHECK(a.texture.maps[1].file == a.texture.maps[2].file && a.texture.maps[2].file == a.texture.maps[3].file);
}

} // namespace

int textureMapsTests() {
  failures = 0;
  mapsTests();
  packedMapsTests();
  return failures;
}
