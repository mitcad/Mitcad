// SPDX-License-Identifier: MIT
#include "Appearances.hpp"

#include <algorithm>
#include <array>
#include <cmath>

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

void resolveTexture(AppearanceTexture& texture, const QString& documentFolder,
                    const std::function<QByteArray()>& embeddedImage) {
  texture.file.clear();
  texture.missing.clear();
  if (texture.embedded) {
    // Embedded images of this session, by digest.
    static QTemporaryDir folder;
    if (!folder.isValid()) {
      texture.missing = QObject::tr("no temporary folder for the embedded image");
      return;
    }
    const QString base = folder.filePath(texture.imageSha256);
    for (const char* extension : {".png", ".jpg"}) {
      if (QFileInfo::exists(base + QLatin1String(extension))) {
        texture.file = base + QLatin1String(extension);
        return;
      }
    }
    const QByteArray bytes = embeddedImage ? embeddedImage() : QByteArray();
    if (bytes.isEmpty()) {
      texture.missing = QObject::tr("the embedded image cannot be read");
      return;
    }
    const QString path = base + (bytes.startsWith("\x89PNG") ? QStringLiteral(".png") : QStringLiteral(".jpg"));
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly) || file.write(bytes) != bytes.size() || !file.commit()) {
      texture.missing = QObject::tr("the embedded image cannot be written to %1").arg(path);
      return;
    }
    texture.file = path;
    return;
  }
  if (texture.path.isEmpty()) {
    texture.missing = QObject::tr("no image is chosen");
    return;
  }
  QString path = texture.path;
  if (QFileInfo(path).isRelative()) {
    if (documentFolder.isEmpty()) {
      texture.missing = QObject::tr("the image %1 is relative to the design's folder, and the design is not saved yet")
                            .arg(texture.path);
      return;
    }
    path = QDir::cleanPath(QDir(documentFolder).filePath(path));
  }
  if (!QFileInfo(path).isFile()) {
    texture.missing = QObject::tr("the image %1 is missing").arg(QDir::toNativeSeparators(path));
    return;
  }
  texture.file = path;
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

const QVector<QPair<QString, QString>>& physicalMaterials() {
  // core/model/src/analysis.rs, MATERIALS.
  static const QVector<QPair<QString, QString>> list = {
      {QStringLiteral("steel"), QObject::tr("Steel")},
      {QStringLiteral("stainless_steel"), QObject::tr("Stainless Steel")},
      {QStringLiteral("cast_iron"), QObject::tr("Cast Iron")},
      {QStringLiteral("aluminum"), QObject::tr("Aluminum")},
      {QStringLiteral("copper"), QObject::tr("Copper")},
      {QStringLiteral("brass"), QObject::tr("Brass")},
      {QStringLiteral("titanium"), QObject::tr("Titanium")},
      {QStringLiteral("abs"), QObject::tr("ABS Plastic")},
      {QStringLiteral("pla"), QObject::tr("PLA Plastic")},
      {QStringLiteral("nylon"), QObject::tr("Nylon")},
      {QStringLiteral("polycarbonate"), QObject::tr("Polycarbonate")},
      {QStringLiteral("glass"), QObject::tr("Glass")},
      {QStringLiteral("water"), QObject::tr("Water")},
  };
  return list;
}

} // namespace mitcad
