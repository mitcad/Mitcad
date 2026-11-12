// SPDX-License-Identifier: MIT
#include "Appearances.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <iterator>

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QObject>
#include <QSaveFile>
#include <QTemporaryDir>

namespace mitcad {

namespace {

using Rgb = std::array<double, 3>;

double toLinear(double c) { return c <= 0.04045 ? c / 12.92 : std::pow((c + 0.055) / 1.055, 2.4); }
double toSrgb(double c) {
  c = std::clamp(c, 0.0, 1.0);
  return c <= 0.0031308 ? c * 12.92 : 1.055 * std::pow(c, 1.0 / 2.4) - 0.055;
}
Rgb linearOf(const QColor& color) { return {toLinear(color.redF()), toLinear(color.greenF()), toLinear(color.blueF())}; }
Rgb operator*(const Rgb& a, double s) { return {a[0] * s, a[1] * s, a[2] * s}; }
Rgb operator*(const Rgb& a, const Rgb& b) { return {a[0] * b[0], a[1] * b[1], a[2] * b[2]}; }
Rgb operator+(const Rgb& a, const Rgb& b) { return {a[0] + b[0], a[1] + b[1], a[2] + b[2]}; }
Rgb mix(const Rgb& a, const Rgb& b, double t) { return a * (1.0 - t) + b * t; }
Rgb grey(double v) { return {v, v, v}; }

struct Vec {
  double x, y, z;
};
double dot(const Vec& a, const Vec& b) { return a.x * b.x + a.y * b.y + a.z * b.z; }
Vec normalized(const Vec& v) {
  const double length = std::sqrt(dot(v, v));
  return {v.x / length, v.y / length, v.z / length};
}

// The swatch's world: a sky above (bluish white), a darker ground below.
Rgb environment(const Vec& direction, double roughness) {
  const double t = std::clamp(0.5 + 0.5 * direction.y, 0.0, 1.0);
  const Rgb sharp = mix(Rgb{0.18, 0.17, 0.16}, Rgb{0.85, 0.9, 1.0}, std::pow(t, 0.7));
  // A rough surface sees the average of the world.
  return mix(sharp, grey(0.5), std::clamp(roughness * 1.4, 0.0, 1.0));
}

// The checkerboard behind the sphere, in linear light.
Rgb checker(double x, double y) {
  const bool dark = (static_cast<int>(std::floor(x * 6.0)) + static_cast<int>(std::floor(y * 6.0))) % 2 != 0;
  return grey(dark ? 0.32 : 0.62);
}

double schlick(double f0, double cosine) { return f0 + (1.0 - f0) * std::pow(1.0 - std::clamp(cosine, 0.0, 1.0), 5.0); }

// A normalized Blinn-Phong highlight standing in for the microfacet lobe.
double highlight(const Vec& normal, const Vec& half, double roughness, double lit) {
  const double alpha = std::max(0.03, roughness * roughness);
  const double shininess = std::min(4000.0, 2.0 / (alpha * alpha) - 2.0);
  return (shininess + 8.0) / 25.0 * std::pow(std::max(0.0, dot(normal, half)), shininess) * lit;
}

} // namespace

bool Appearance::sameLook(const Appearance& o) const {
  return baseColor == o.baseColor && metalness == o.metalness && roughness == o.roughness && specular == o.specular &&
         transmission == o.transmission && ior == o.ior && coat == o.coat && coatRoughness == o.coatRoughness &&
         emission == o.emission && emissionColor == o.emissionColor && opacity == o.opacity &&
         hasTexture == o.hasTexture && texture == o.texture && displayColor == o.displayColor;
}

namespace {

// The file of one image of a texture (resolveTexture): sets `file`, or
// `missing` with the reason.
void resolveImage(const QString& imagePath, bool embedded, const QString& sha256, const QString& documentFolder,
                  const std::function<QByteArray()>& embeddedImage, QString& file, QString& missing) {
  file.clear();
  missing.clear();
  if (embedded) {
    // Embedded images of this session, by digest (shared by the maps of
    // every appearance that uses the same image).
    static QTemporaryDir folder;
    if (!folder.isValid()) {
      missing = QObject::tr("no temporary folder for the embedded image");
      return;
    }
    const QString base = folder.filePath(sha256);
    for (const char* extension : {".png", ".jpg"}) {
      if (!sha256.isEmpty() && QFileInfo::exists(base + QLatin1String(extension))) {
        file = base + QLatin1String(extension);
        return;
      }
    }
    const QByteArray bytes = embeddedImage ? embeddedImage() : QByteArray();
    if (bytes.isEmpty() || sha256.isEmpty()) {
      missing = QObject::tr("the embedded image cannot be read");
      return;
    }
    const QString path = base + (bytes.startsWith("\x89PNG") ? QStringLiteral(".png") : QStringLiteral(".jpg"));
    QSaveFile saved(path);
    if (!saved.open(QIODevice::WriteOnly) || saved.write(bytes) != bytes.size() || !saved.commit()) {
      missing = QObject::tr("the embedded image cannot be written to %1").arg(path);
      return;
    }
    file = path;
    return;
  }
  if (imagePath.isEmpty()) {
    missing = QObject::tr("no image is chosen");
    return;
  }
  QString path = imagePath;
  if (QFileInfo(path).isRelative()) {
    if (documentFolder.isEmpty()) {
      missing = QObject::tr("the image %1 is relative to the design's folder, and the design is not saved yet")
                    .arg(imagePath);
      return;
    }
    path = QDir::cleanPath(QDir(documentFolder).filePath(path));
  }
  if (!QFileInfo(path).isFile()) {
    missing = QObject::tr("the image %1 is missing").arg(QDir::toNativeSeparators(path));
    return;
  }
  file = path;
}

} // namespace

const std::array<const char*, kTextureMapCount>& textureMapNames() {
  static const std::array<const char*, kTextureMapCount> names = {"normal", "roughness", "metalness", "occlusion"};
  return names;
}

void resolveTexture(AppearanceTexture& texture, const QString& documentFolder,
                    const std::function<QByteArray()>& embeddedImage) {
  resolveImage(texture.path, texture.embedded, texture.imageSha256, documentFolder, embeddedImage, texture.file,
               texture.missing);
}

void resolveTextureMaps(AppearanceTexture& texture, const QString& documentFolder,
                        const std::function<QByteArray(const QString& map)>& embeddedImage) {
  const auto bytesOf = [&embeddedImage](const QString& map) -> std::function<QByteArray()> {
    if (!embeddedImage) {
      return {};
    }
    return [&embeddedImage, map] { return embeddedImage(map); };
  };
  resolveImage(texture.path, texture.embedded, texture.imageSha256, documentFolder,
               bytesOf(QStringLiteral("base_color")), texture.file, texture.missing);
  for (std::size_t i = 0; i < texture.maps.size(); ++i) {
    AppearanceMap& map = texture.maps[i];
    map.file.clear();
    map.missing.clear();
    if (map.present) {
      resolveImage(map.path, map.embedded, map.imageSha256, documentFolder,
                   bytesOf(QLatin1String(textureMapNames()[i])), map.file, map.missing);
    }
  }
}

QJsonArray colorJson(const QColor& color) { return QJsonArray{color.redF(), color.greenF(), color.blueF()}; }

QColor colorOf(const QJsonValue& value) {
  const QJsonArray rgb = value.toArray();
  if (rgb.size() != 3) {
    return QColor();
  }
  return QColor::fromRgbF(static_cast<float>(rgb.at(0).toDouble()), static_cast<float>(rgb.at(1).toDouble()),
                          static_cast<float>(rgb.at(2).toDouble()));
}

Appearance appearanceOf(const QJsonObject& value) {
  Appearance a;
  a.id = value.value(QStringLiteral("id")).toString();
  a.name = value.value(QStringLiteral("name")).toString();
  a.library = value.value(QStringLiteral("library")).toBool(true);
  // Its class (mitcad#1102): its own, else the library's by its id, else
  // other; empty from a model before classes.
  a.materialClass = value.value(QStringLiteral("class")).toString();
  a.baseColor = colorOf(value.value(QStringLiteral("base_color")));
  const auto number = [&value](const char* key, double fallback) {
    return value.value(QLatin1String(key)).toDouble(fallback);
  };
  a.metalness = number("metalness", a.metalness);
  a.roughness = number("roughness", a.roughness);
  a.specular = number("specular", a.specular);
  a.transmission = number("transmission", a.transmission);
  a.ior = number("ior", a.ior);
  a.coat = number("coat", a.coat);
  a.coatRoughness = number("coat_roughness", a.coatRoughness);
  a.emission = number("emission", a.emission);
  a.emissionColor = colorOf(value.value(QStringLiteral("emission_color")));
  a.opacity = number("opacity", a.opacity);
  a.hasTexture = value.value(QStringLiteral("texture")).isObject();
  if (a.hasTexture) {
    const QJsonObject texture = value.value(QStringLiteral("texture")).toObject();
    a.texture.json = texture;
    a.texture.path = texture.value(QStringLiteral("path")).toString();
    const QJsonArray size = texture.value(QStringLiteral("size")).toArray();
    a.texture.width = size.at(0).toDouble(100.0);
    a.texture.height = size.at(1).toDouble(100.0);
    a.texture.rotation = texture.value(QStringLiteral("rotation")).toDouble();
    a.texture.planar = texture.value(QStringLiteral("projection")).toString() == QLatin1String("planar");
    a.texture.embedded = texture.value(QStringLiteral("embedded")).toBool();
    a.texture.imageSha256 = texture.value(QStringLiteral("image_sha256")).toString();
    // The other maps of the set (mitcad#313).
    for (std::size_t i = 0; i < a.texture.maps.size(); ++i) {
      const QJsonValue listed = texture.value(QLatin1String(textureMapNames()[i]));
      AppearanceMap& map = a.texture.maps[i];
      map.present = listed.isObject();
      if (map.present) {
        const QJsonObject object = listed.toObject();
        map.path = object.value(QStringLiteral("path")).toString();
        map.embedded = object.value(QStringLiteral("embedded")).toBool();
        map.imageSha256 = object.value(QStringLiteral("image_sha256")).toString();
      }
    }
  }
  a.displayColor = colorOf(value.value(QStringLiteral("display_color")));
  if (!a.displayColor.isValid()) {
    a.displayColor = a.baseColor;
  }
  for (const QJsonValue& body : value.value(QStringLiteral("bodies")).toArray()) {
    a.bodies << body.toString();
  }
  for (const QJsonValue& face : value.value(QStringLiteral("faces")).toArray()) {
    a.faces.append({face.toObject().value(QStringLiteral("body")).toString(),
                    face.toObject().value(QStringLiteral("face")).toString()});
  }
  return a;
}

QVector<Appearance> appearancesOf(const QJsonArray& query) {
  QVector<Appearance> list;
  list.reserve(query.size());
  for (const QJsonValue& value : query) {
    list.append(appearanceOf(value.toObject()));
  }
  return list;
}

QJsonObject appearanceFields(const Appearance& a) {
  return {{QStringLiteral("name"), a.name},
          {QStringLiteral("base_color"), colorJson(a.baseColor)},
          {QStringLiteral("metalness"), a.metalness},
          {QStringLiteral("roughness"), a.roughness},
          {QStringLiteral("specular"), a.specular},
          {QStringLiteral("transmission"), a.transmission},
          {QStringLiteral("ior"), a.ior},
          {QStringLiteral("coat"), a.coat},
          {QStringLiteral("coat_roughness"), a.coatRoughness},
          {QStringLiteral("emission"), a.emission},
          {QStringLiteral("emission_color"), colorJson(a.emissionColor)},
          {QStringLiteral("opacity"), a.opacity}};
}

const Appearance* findAppearance(const QVector<Appearance>& appearances, const QString& id) {
  for (const Appearance& appearance : appearances) {
    if (appearance.id == id) {
      return &appearance;
    }
  }
  return nullptr;
}

QHash<QString, Appearance> appearancesById(const QVector<Appearance>& appearances) {
  QHash<QString, Appearance> byId;
  for (const Appearance& appearance : appearances) {
    byId.insert(appearance.id, appearance);
  }
  return byId;
}

QImage appearanceSwatch(const Appearance& a, int size, qreal devicePixelRatio) {
  const int n = std::max(1, static_cast<int>(std::lround(size * devicePixelRatio)));
  QImage image(n, n, QImage::Format_ARGB32);
  image.setDevicePixelRatio(devicePixelRatio);
  const Rgb base = linearOf(a.baseColor);
  const Rgb glow = linearOf(a.emissionColor) * a.emission;
  const Vec light = normalized({-0.55, 0.65, 0.55});
  const Vec view{0.0, 0.0, 1.0};
  const Vec half = normalized({light.x + view.x, light.y + view.y, light.z + view.z});
  const double radius = 0.9;
  const double dielectric = std::pow((a.ior - 1.0) / (a.ior + 1.0), 2.0) * a.specular;
  for (int py = 0; py < n; ++py) {
    QRgb* row = reinterpret_cast<QRgb*>(image.scanLine(py));
    for (int px = 0; px < n; ++px) {
      const double x = (px + 0.5) / n;
      const double y = (py + 0.5) / n;
      const Rgb behind = checker(x, y);
      const double u = (2.0 * x - 1.0) / radius;
      const double v = (1.0 - 2.0 * y) / radius;
      const double r2 = u * u + v * v;
      // The sphere's edge antialiased over about a pixel.
      const double coverage = std::clamp((1.0 - std::sqrt(r2)) * radius * n / 2.0 + 0.5, 0.0, 1.0);
      Rgb color = behind;
      if (coverage > 0.0) {
        const double z = std::sqrt(std::max(0.0, 1.0 - std::min(r2, 1.0)));
        const Vec normal{u, v, z};
        const double lit = std::max(0.0, dot(normal, light));
        const double facing = std::max(0.0, z);
        const Vec mirrored{2.0 * facing * normal.x, 2.0 * facing * normal.y, 2.0 * facing * normal.z - 1.0};
        const Rgb world = environment(mirrored, a.roughness);
        const double spot = highlight(normal, half, a.roughness, lit);
        // Metal: a coloured reflection of the world and the light.
        const Rgb metalF{schlick(base[0], facing), schlick(base[1], facing), schlick(base[2], facing)};
        const Rgb metal = metalF * (world + grey(spot));
        // Dielectric: diffuse and transmitted light under a specular layer.
        const double fresnel = schlick(dielectric, facing);
        const Rgb diffuse = base * (0.18 + 0.8 * lit);
        // What is behind, bent by the refraction and tinted twice (in and out).
        const double bend = 0.25 * (a.ior - 1.0);
        const Rgb seen = checker(x - normal.x * bend * radius / 2.0, y + normal.y * bend * radius / 2.0);
        const Rgb transmitted = mix(seen, environment({0.0, -1.0, 0.0}, a.roughness), a.roughness * 0.7) * base * base;
        const Rgb under = mix(diffuse, transmitted, a.transmission);
        const Rgb plastic = under * (1.0 - fresnel) + (world + grey(spot)) * fresnel;
        Rgb surface = mix(plastic, metal, a.metalness);
        if (a.coat > 0.0) {
          const double coated = a.coat * schlick(0.04, facing);
          const Rgb coatWorld = environment(mirrored, a.coatRoughness) +
                                grey(highlight(normal, half, a.coatRoughness, lit));
          surface = surface * (1.0 - coated) + coatWorld * coated;
        }
        surface = surface + glow;
        color = mix(behind, surface, a.opacity * coverage);
      }
      row[px] = qRgb(static_cast<int>(std::lround(toSrgb(color[0]) * 255.0)),
                     static_cast<int>(std::lround(toSrgb(color[1]) * 255.0)),
                     static_cast<int>(std::lround(toSrgb(color[2]) * 255.0)));
    }
  }
  return image;
}

namespace {

// A material of MitCad's own by its id and its English name, as the model
// and the library store it.
struct OwnMaterialName {
  const char* id;
  const char* name;
};

// The built-in materials (core/model-base/src/analysis.rs, MATERIALS), in
// their order, steel first as the default. Their names are translated in
// the context MaterialName (mitcad#1689).
constexpr OwnMaterialName kBuiltInMaterials[] = {
    {"steel", QT_TRANSLATE_NOOP("MaterialName", "Steel")},
    {"stainless_steel", QT_TRANSLATE_NOOP("MaterialName", "Stainless Steel")},
    {"cast_iron", QT_TRANSLATE_NOOP("MaterialName", "Cast Iron")},
    {"aluminum", QT_TRANSLATE_NOOP("MaterialName", "Aluminum")},
    {"copper", QT_TRANSLATE_NOOP("MaterialName", "Copper")},
    {"brass", QT_TRANSLATE_NOOP("MaterialName", "Brass")},
    {"titanium", QT_TRANSLATE_NOOP("MaterialName", "Titanium")},
    {"abs", QT_TRANSLATE_NOOP("MaterialName", "ABS Plastic")},
    {"pla", QT_TRANSLATE_NOOP("MaterialName", "PLA Plastic")},
    {"nylon", QT_TRANSLATE_NOOP("MaterialName", "Nylon")},
    {"polycarbonate", QT_TRANSLATE_NOOP("MaterialName", "Polycarbonate")},
    {"glass", QT_TRANSLATE_NOOP("MaterialName", "Glass")},
    {"water", QT_TRANSLATE_NOOP("MaterialName", "Water")},
    {"resin_standard", QT_TRANSLATE_NOOP("MaterialName", "Resin - Standard")},
};

// The built-in woods and panels after them (mitcad#1706,
// core/model/src/material.rs, BUILT_IN_WOODS): not printed, so not among
// the print analysis' materials.
constexpr OwnMaterialName kBuiltInWoods[] = {
    {"wood_pine", QT_TRANSLATE_NOOP("MaterialName", "Wood - Pine")},
    {"wood_oak", QT_TRANSLATE_NOOP("MaterialName", "Wood - Oak")},
    {"wood_birch", QT_TRANSLATE_NOOP("MaterialName", "Wood - Birch")},
    {"plywood_birch", QT_TRANSLATE_NOOP("MaterialName", "Plywood - birch")},
    {"mdf_raw", QT_TRANSLATE_NOOP("MaterialName", "MDF - raw")},
};

// The base library's materials whose names have words to translate
// (tools/libraries/print-library/materials); the others are named by their
// filament types alone (ABS, ASA, PETG, PLA, TPU), the same in every
// language.
constexpr OwnMaterialName kBaseLibraryMaterials[] = {
    {"nylon", QT_TRANSLATE_NOOP("MaterialName", "PA (Nylon)")},
};

QString translatedMaterialName(const char* name) { return QCoreApplication::translate("MaterialName", name); }

} // namespace

const QVector<QPair<QString, QString>>& physicalMaterials() {
  static const QVector<QPair<QString, QString>> list = [] {
    QVector<QPair<QString, QString>> made;
    for (const OwnMaterialName& material : kBuiltInMaterials) {
      made.append({QString::fromLatin1(material.id), translatedMaterialName(material.name)});
    }
    for (const OwnMaterialName& material : kBuiltInWoods) {
      made.append({QString::fromLatin1(material.id), translatedMaterialName(material.name)});
    }
    return made;
  }();
  return list;
}

bool isBuiltInWood(const QString& id) {
  return std::any_of(std::begin(kBuiltInWoods), std::end(kBuiltInWoods),
                     [&id](const OwnMaterialName& material) { return id == QLatin1String(material.id); });
}

namespace {

// The English name of MitCad's own material `name` (with the id `id`, when
// one is given) as the language shows it; null when it is none of them.
const char* ownMaterialName(const QString* id, const QString& name) {
  for (const OwnMaterialName& material : kBuiltInMaterials) {
    if ((id == nullptr || *id == QLatin1String(material.id)) && name == QLatin1String(material.name)) {
      return material.name;
    }
  }
  for (const OwnMaterialName& material : kBuiltInWoods) {
    if ((id == nullptr || *id == QLatin1String(material.id)) && name == QLatin1String(material.name)) {
      return material.name;
    }
  }
  for (const OwnMaterialName& material : kBaseLibraryMaterials) {
    if ((id == nullptr || *id == QLatin1String(material.id)) && name == QLatin1String(material.name)) {
      return material.name;
    }
  }
  return nullptr;
}

} // namespace

QString shownMaterialName(const QString& id, const QString& name) {
  const char* own = ownMaterialName(&id, name);
  return own != nullptr ? translatedMaterialName(own) : name;
}

QString shownOwnMaterialName(const QString& name) {
  const char* own = ownMaterialName(nullptr, name);
  return own != nullptr ? translatedMaterialName(own) : name;
}

QString shownLibraryMaterialName(const QString& place, const QString& sourcePlace, const QString& id,
                                 const QString& name) {
  if (place == QLatin1String("mitcad") || sourcePlace == QLatin1String("mitcad")) {
    return shownOwnMaterialName(name);
  }
  return place == QLatin1String("design") ? shownMaterialName(id, name) : name;
}

} // namespace mitcad
